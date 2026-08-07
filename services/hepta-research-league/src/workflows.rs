use std::collections::{BTreeMap, HashSet};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use trnm_finality_types::{
    FinalityReceiptV1 as LiveFinalityReceiptV1, ValidatorDescriptorV1,
    ValidatorSetV1 as LiveValidatorSetV1,
};
use trnm_finality_verifier::verify_finality_receipt as verify_live_finality_receipt;
use uuid::Uuid;

use super::{
    push_event, require_service_token, sha256_hex, validate_hash, validate_non_empty, ApiError,
    AppState, ChallengeStatus, EventEnvelope, FinalityMode, NAKAMA_TOKEN_HEADER,
    OPERATOR_TOKEN_HEADER, TRNM_TOKEN_HEADER,
};
use crate::trnm_v1::{
    command_kind, format_digest, verify_finality_receipt, AuthorityRole, FinalityReceiptV1,
    ObjectInclusionProofV1, ObjectRefV1, QuorumCertificateV1, SignedResearchCommandV1,
    TrnmCommandKind, VerifiedFinalityV1,
};

pub const EVALUATOR_MANIFEST_V1: &str = "hepta_evaluator_manifest_v1";
pub const EVALUATION_REPORT_V1: &str = "hepta_evaluation_report_v1";
pub const REPRODUCTION_REPORT_V1: &str = "hepta_reproduction_report_v1";
pub const TRNM_ADAPTER_V2: &str = "hepta_trnm_adapter_v2";
pub const NAKAMA_EVENT_V2: &str = "hepta_nakama_match_event_v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationCriterion {
    pub metric: String,
    pub weight_bps: u16,
    pub minimum_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluatorManifest {
    pub evaluator_manifest_id: Uuid,
    pub challenge_id: Uuid,
    pub version: String,
    pub protocol: String,
    pub criteria: Vec<EvaluationCriterion>,
    pub manifest_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateEvaluatorManifestRequest {
    challenge_id: Uuid,
    version: String,
    criteria: Vec<EvaluationCriterion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    Accepted,
    Rejected,
    SupersededOnAppeal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationReport {
    pub evaluation_report_id: Uuid,
    pub submission_id: Uuid,
    pub evaluator_manifest_id: Uuid,
    pub evaluator_manifest_hash: String,
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub score_micros: i64,
    pub constraints_satisfied: bool,
    pub status: EvaluationStatus,
    pub report_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateEvaluationRequest {
    submission_id: Uuid,
    evaluator_manifest_id: Uuid,
    observed_metrics_micros: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReproductionReport {
    pub reproduction_report_id: Uuid,
    pub evaluation_report_id: Uuid,
    pub reproducer_id: String,
    pub environment_hash: String,
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub reproduced: bool,
    pub report_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateReproductionRequest {
    evaluation_report_id: Uuid,
    reproducer_id: String,
    environment_hash: String,
    observed_metrics_micros: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppealStatus {
    Open,
    Upheld,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppealCase {
    pub appeal_id: Uuid,
    pub evaluation_report_id: Uuid,
    pub appellant_id: String,
    pub grounds_hash: String,
    pub status: AppealStatus,
    pub resolution_hash: Option<String>,
    pub adjusted_score_micros: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct CreateAppealRequest {
    evaluation_report_id: Uuid,
    appellant_id: String,
    grounds_hash: String,
}

#[derive(Debug, Deserialize)]
struct ResolveAppealRequest {
    outcome: AppealStatus,
    resolution_hash: String,
    adjusted_score_micros: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrnmCommand {
    pub command_id: Uuid,
    pub kind: TrnmCommandKind,
    pub aggregate_id: String,
    pub idempotency_key: String,
    pub command_fingerprint: String,
    #[serde(default)]
    pub paper_binding: Option<crate::PaperTrnmCommandBindingV1>,
    #[serde(default)]
    pub paper_binding_fingerprint: Option<String>,
    pub signed_command: SignedResearchCommandV1,
    pub status: TrnmProjectionStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrnmProjectionStatus {
    PendingFinality,
    VerifiedFinality,
    Provisional,
    Finalized,
    Challenged,
    Resolved,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTrnmCommandRequest {
    signed_command: SignedResearchCommandV1,
    #[serde(default)]
    paper_binding: Option<crate::PaperTrnmCommandBindingV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrnmFinalityProjection {
    pub command_id: Uuid,
    pub source_event_id: Uuid,
    pub chain_id: String,
    pub tx_hash: String,
    pub tx_index: u64,
    pub block_height: u64,
    pub block_hash: String,
    pub state_root: String,
    pub object_ref: ObjectRefV1,
    pub inclusion_proof: ObjectInclusionProofV1,
    pub validator_set_id: String,
    pub quorum_certificate: QuorumCertificateV1,
    pub confirmations: u64,
    pub status: TrnmProjectionStatus,
    pub receipt_hash: String,
    pub verified: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveTrnmFinalityRequestV1 {
    pub source_event_id: Uuid,
    pub receipt: LiveFinalityReceiptV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveTrnmFinalityProjection {
    pub command_id: Uuid,
    pub source_event_id: Uuid,
    pub command_fingerprint: String,
    pub receipt: LiveFinalityReceiptV1,
    pub status: TrnmProjectionStatus,
    pub verified: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NakamaMatchStatus {
    Started,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NakamaMatchProjection {
    pub match_id: Uuid,
    pub challenge_id: Uuid,
    pub status: NakamaMatchStatus,
    pub last_sequence: u64,
    pub event_hashes: Vec<String>,
    pub event_root: Option<String>,
    pub archive_uri: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct NakamaMatchEvent {
    protocol: String,
    event_id: Uuid,
    event_type: String,
    match_id: Uuid,
    challenge_id: Uuid,
    sequence: u64,
    event_hash: String,
    event_root: Option<String>,
    archive_uri: Option<String>,
    occurred_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ReconciliationView {
    match_id: Uuid,
    status: NakamaMatchStatus,
    sequence_contiguous: bool,
    event_root: Option<String>,
    computed_event_root: String,
    reconciled: bool,
}

#[derive(Debug, Serialize)]
struct ReadyResponse {
    ready: bool,
    storage: &'static str,
    database: &'static str,
    security: &'static str,
    nakama_control: &'static str,
    failures: Vec<&'static str>,
    agent_execution_mode: &'static str,
    top_level_modules: [&'static str; 3],
    finality_mode: &'static str,
    trusted_validator_sets: usize,
    pinned_cometbft_trust_anchor_hashes: usize,
    trnm_receipt_v2_max_body_bytes: usize,
    trnm_receipt_v2_max_in_flight: usize,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/v1/hepta/openapi.yaml", get(openapi))
        .route(
            "/v1/hepta/evaluator-manifests",
            post(create_evaluator_manifest),
        )
        .route(
            "/v1/hepta/evaluator-manifests/:manifest_id",
            get(get_evaluator_manifest),
        )
        .route("/v1/hepta/evaluations", post(create_evaluation))
        .route("/v1/hepta/evaluations/:evaluation_id", get(get_evaluation))
        .route("/v1/hepta/reproductions", post(create_reproduction))
        .route("/v1/hepta/appeals", post(create_appeal))
        .route("/v1/hepta/appeals/:appeal_id/resolve", post(resolve_appeal))
        .route(
            "/v1/hepta/trnm/commitments",
            post(create_evaluation_commitment),
        )
        .route(
            "/v1/hepta/trnm/evaluation-commitments",
            post(create_evaluation_commitment),
        )
        .route(
            "/v1/hepta/trnm/workload-receipts",
            post(create_workload_receipt),
        )
        .route(
            "/v1/hepta/trnm/research-claims",
            post(create_research_claim),
        )
        .route(
            "/v1/hepta/trnm/license-declarations",
            post(create_license_declaration),
        )
        .route(
            "/v1/hepta/trnm/claim-challenges",
            post(create_claim_challenge),
        )
        .route(
            "/v1/hepta/trnm/challenge-resolutions",
            post(create_challenge_resolution),
        )
        .route("/v1/hepta/trnm/finality", post(ingest_trnm_finality))
        .route(
            "/v1/hepta/trnm/finality/live",
            post(ingest_live_trnm_finality),
        )
        .route(
            "/v1/hepta/trnm/finality/live/:command_id",
            get(get_live_trnm_finality),
        )
        .route(
            "/v1/hepta/trnm/finality/verify",
            post(verify_trnm_finality_offline),
        )
        .route(
            "/v1/hepta/trnm/finality/:command_id",
            get(get_trnm_finality),
        )
        .route("/v1/hepta/nakama/events", post(ingest_nakama_event))
        .route(
            "/v1/hepta/nakama/matches/:match_id/reconciliation",
            get(reconcile_nakama_match),
        )
        .route(
            "/v1/hepta/research-terminal/challenges/:challenge_id/task-package",
            get(task_package),
        )
        .route(
            "/v1/hepta/research-terminal/agents/:agent_id/profile",
            get(agent_profile),
        )
        .route("/v1/hepta/operator/outbox", get(operator_outbox))
}

async fn openapi() -> &'static str {
    include_str!("../../../docs/openapi/hepta-research-league-v1.yaml")
}

async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadyResponse>) {
    let mut failures = state.security.readiness_errors();
    let nakama_control = if state.nakama_control_http_configured() {
        "configured"
    } else {
        failures.push("nakama_control_http_missing");
        "missing"
    };
    let database = if let Some(pool) = &state.pool {
        match pool.acquire().await {
            Ok(mut connection) => match sqlx::query_scalar::<_, i32>("select 1")
                .fetch_one(&mut *connection)
                .await
            {
                Ok(1) => "reachable",
                Ok(_) => {
                    failures.push("database_probe_unexpected_result");
                    "unreachable"
                }
                Err(_) => {
                    failures.push("database_probe_failed");
                    "unreachable"
                }
            },
            Err(_) => {
                failures.push("database_pool_acquire_failed");
                "unreachable"
            }
        }
    } else {
        failures.push("database_pool_missing");
        "not_configured"
    };
    let ready = failures.is_empty();
    let response = ReadyResponse {
        ready,
        storage: if state.is_durable() {
            "postgresql"
        } else {
            "in_memory_test_only"
        },
        database,
        security: if state.security.readiness_errors().is_empty() {
            "valid"
        } else {
            "invalid"
        },
        nakama_control,
        failures,
        agent_execution_mode: "external_only",
        top_level_modules: ["hepta", "nakama", "trnm"],
        finality_mode: state.security.finality_mode.as_str(),
        trusted_validator_sets: state.security.trusted_trnm_validator_sets.len(),
        pinned_cometbft_trust_anchor_hashes: state
            .security
            .pinned_trnm_cometbft_trust_anchor_hashes
            .len(),
        trnm_receipt_v2_max_body_bytes: state.security.trnm_receipt_v2_max_body_bytes,
        trnm_receipt_v2_max_in_flight: state.security.trnm_receipt_v2_max_in_flight,
    };
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(response),
    )
}

fn require_verified_finality_mode(state: &AppState) -> Result<(), ApiError> {
    if state.security.finality_mode != FinalityMode::Verified {
        return Err(ApiError::conflict(
            "finality_pending_only",
            "Hepta is configured for pending_only finality; receipts remain held and no finality write is accepted",
        ));
    }
    if state.security.trusted_trnm_validator_sets.is_empty() {
        return Err(ApiError::internal(
            "verified finality mode has no trusted validator sets",
        ));
    }
    Ok(())
}

async fn metrics(State(state): State<AppState>) -> Result<String, ApiError> {
    let mut metrics = state
        .inspect(|league| {
            let pending = league
                .trnm_commands
                .values()
                .filter(|command| command.status == TrnmProjectionStatus::PendingFinality)
                .count();
            Ok(format!(
                "# HELP hepta_up Whether Hepta metrics are served.\n\
                 # TYPE hepta_up gauge\nhepta_up 1\n\
                 # TYPE hepta_agents gauge\nhepta_agents {}\n\
                 # TYPE hepta_submissions gauge\nhepta_submissions {}\n\
                 # TYPE hepta_evaluations gauge\nhepta_evaluations {}\n\
                 # TYPE hepta_trnm_pending_finality gauge\nhepta_trnm_pending_finality {}\n\
                 # TYPE hepta_nakama_matches gauge\nhepta_nakama_matches {}\n",
                league.agents.len(),
                league.submissions.len(),
                league.evaluation_reports.len(),
                pending,
                league.nakama_matches.len()
            ))
        })
        .await?;
    metrics.push_str(&crate::paper_raid_v2::operational_metrics(&state).await?);
    Ok(metrics)
}

async fn create_evaluator_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEvaluatorManifestRequest>,
) -> Result<(StatusCode, Json<EvaluatorManifest>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    validate_non_empty("version", &request.version)?;
    validate_criteria(&request.criteria)?;
    state
        .transact(|league| {
            if !league.challenges.contains_key(&request.challenge_id) {
                return Err(ApiError::not_found(
                    "challenge_not_found",
                    format!("challenge {} does not exist", request.challenge_id),
                ));
            }
            if let Some(existing) = league.evaluator_manifests.values().find(|manifest| {
                manifest.challenge_id == request.challenge_id && manifest.version == request.version
            }) {
                return Ok((StatusCode::OK, Json(existing.clone())));
            }
            let hash_payload = json!({
                "protocol": EVALUATOR_MANIFEST_V1,
                "challenge_id": request.challenge_id,
                "version": request.version,
                "criteria": request.criteria,
            });
            let manifest = EvaluatorManifest {
                evaluator_manifest_id: Uuid::new_v4(),
                challenge_id: request.challenge_id,
                version: request.version,
                protocol: EVALUATOR_MANIFEST_V1.to_string(),
                criteria: request.criteria,
                manifest_hash: hash_value(&hash_payload),
                created_at: Utc::now(),
            };
            league
                .evaluator_manifests
                .insert(manifest.evaluator_manifest_id, manifest.clone());
            push_event(
                league,
                "hepta.evaluator_manifest.created.v1",
                manifest.challenge_id.to_string(),
                serde_json::to_value(&manifest).expect("serialize evaluator manifest"),
            );
            Ok((StatusCode::CREATED, Json(manifest)))
        })
        .await
}

async fn get_evaluator_manifest(
    State(state): State<AppState>,
    Path(manifest_id): Path<Uuid>,
) -> Result<Json<EvaluatorManifest>, ApiError> {
    state
        .inspect(|league| {
            league
                .evaluator_manifests
                .get(&manifest_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "evaluator_manifest_not_found",
                        format!("evaluator manifest {manifest_id} does not exist"),
                    )
                })
        })
        .await
}

async fn create_evaluation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEvaluationRequest>,
) -> Result<(StatusCode, Json<EvaluationReport>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    state
        .transact(|league| {
            let submission = league
                .submissions
                .get(&request.submission_id)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "submission_not_found",
                        format!("submission {} does not exist", request.submission_id),
                    )
                })?;
            let manifest = league
                .evaluator_manifests
                .get(&request.evaluator_manifest_id)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "evaluator_manifest_not_found",
                        format!(
                            "evaluator manifest {} does not exist",
                            request.evaluator_manifest_id
                        ),
                    )
                })?;
            if submission.challenge_id != manifest.challenge_id {
                return Err(ApiError::conflict(
                    "evaluation_challenge_mismatch",
                    "submission and evaluator manifest belong to different challenges",
                ));
            }
            if let Some(existing) = league.evaluation_reports.values().find(|report| {
                report.submission_id == request.submission_id
                    && report.evaluator_manifest_id == request.evaluator_manifest_id
            }) {
                if existing.observed_metrics_micros == request.observed_metrics_micros {
                    return Ok((StatusCode::OK, Json(existing.clone())));
                }
                return Err(ApiError::conflict(
                    "evaluation_conflict",
                    "an immutable evaluation already exists for this submission and manifest",
                ));
            }
            let (score_micros, constraints_satisfied) =
                deterministic_score(manifest, &request.observed_metrics_micros)?;
            let created_at = Utc::now();
            let report_payload = json!({
                "protocol": EVALUATION_REPORT_V1,
                "submission_id": request.submission_id,
                "evaluator_manifest_id": request.evaluator_manifest_id,
                "evaluator_manifest_hash": manifest.manifest_hash,
                "observed_metrics_micros": request.observed_metrics_micros,
                "score_micros": score_micros,
                "constraints_satisfied": constraints_satisfied,
                "created_at": created_at,
            });
            let report = EvaluationReport {
                evaluation_report_id: Uuid::new_v4(),
                submission_id: request.submission_id,
                evaluator_manifest_id: request.evaluator_manifest_id,
                evaluator_manifest_hash: manifest.manifest_hash.clone(),
                observed_metrics_micros: request.observed_metrics_micros,
                score_micros,
                constraints_satisfied,
                status: if constraints_satisfied {
                    EvaluationStatus::Accepted
                } else {
                    EvaluationStatus::Rejected
                },
                report_hash: hash_value(&report_payload),
                created_at,
            };
            league
                .evaluation_reports
                .insert(report.evaluation_report_id, report.clone());
            push_event(
                league,
                "hepta.evaluation.completed.v1",
                report.submission_id.to_string(),
                serde_json::to_value(&report).expect("serialize evaluation report"),
            );
            Ok((StatusCode::CREATED, Json(report)))
        })
        .await
}

async fn get_evaluation(
    State(state): State<AppState>,
    Path(evaluation_id): Path<Uuid>,
) -> Result<Json<EvaluationReport>, ApiError> {
    state
        .inspect(|league| {
            league
                .evaluation_reports
                .get(&evaluation_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "evaluation_not_found",
                        format!("evaluation {evaluation_id} does not exist"),
                    )
                })
        })
        .await
}

async fn create_reproduction(
    State(state): State<AppState>,
    Json(request): Json<CreateReproductionRequest>,
) -> Result<(StatusCode, Json<ReproductionReport>), ApiError> {
    validate_non_empty("reproducer_id", &request.reproducer_id)?;
    validate_hash("environment_hash", &request.environment_hash)?;
    state
        .enforce_rate_limit("reproduction", &request.reproducer_id, 60, 60)
        .await?;
    state
        .transact(|league| {
            let evaluation = league
                .evaluation_reports
                .get(&request.evaluation_report_id)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "evaluation_not_found",
                        format!("evaluation {} does not exist", request.evaluation_report_id),
                    )
                })?;
            let reproduced = evaluation.observed_metrics_micros == request.observed_metrics_micros;
            let created_at = Utc::now();
            let payload = json!({
                "protocol": REPRODUCTION_REPORT_V1,
                "evaluation_report_id": request.evaluation_report_id,
                "reproducer_id": request.reproducer_id,
                "environment_hash": request.environment_hash,
                "observed_metrics_micros": request.observed_metrics_micros,
                "reproduced": reproduced,
                "created_at": created_at,
            });
            let report = ReproductionReport {
                reproduction_report_id: Uuid::new_v4(),
                evaluation_report_id: request.evaluation_report_id,
                reproducer_id: request.reproducer_id,
                environment_hash: request.environment_hash,
                observed_metrics_micros: request.observed_metrics_micros,
                reproduced,
                report_hash: hash_value(&payload),
                created_at,
            };
            league
                .reproduction_reports
                .insert(report.reproduction_report_id, report.clone());
            push_event(
                league,
                "hepta.reproduction.completed.v1",
                report.evaluation_report_id.to_string(),
                serde_json::to_value(&report).expect("serialize reproduction report"),
            );
            Ok((StatusCode::CREATED, Json(report)))
        })
        .await
}

async fn create_appeal(
    State(state): State<AppState>,
    Json(request): Json<CreateAppealRequest>,
) -> Result<(StatusCode, Json<AppealCase>), ApiError> {
    validate_non_empty("appellant_id", &request.appellant_id)?;
    validate_hash("grounds_hash", &request.grounds_hash)?;
    state
        .enforce_rate_limit("appeal", &request.appellant_id, 10, 60)
        .await?;
    state
        .transact(|league| {
            if !league
                .evaluation_reports
                .contains_key(&request.evaluation_report_id)
            {
                return Err(ApiError::not_found(
                    "evaluation_not_found",
                    format!("evaluation {} does not exist", request.evaluation_report_id),
                ));
            }
            let appeal = AppealCase {
                appeal_id: Uuid::new_v4(),
                evaluation_report_id: request.evaluation_report_id,
                appellant_id: request.appellant_id,
                grounds_hash: request.grounds_hash,
                status: AppealStatus::Open,
                resolution_hash: None,
                adjusted_score_micros: None,
                created_at: Utc::now(),
                resolved_at: None,
            };
            league.appeal_cases.insert(appeal.appeal_id, appeal.clone());
            push_event(
                league,
                "hepta.appeal.opened.v1",
                appeal.appeal_id.to_string(),
                serde_json::to_value(&appeal).expect("serialize appeal"),
            );
            Ok((StatusCode::CREATED, Json(appeal)))
        })
        .await
}

async fn resolve_appeal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(appeal_id): Path<Uuid>,
    Json(request): Json<ResolveAppealRequest>,
) -> Result<Json<AppealCase>, ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    validate_hash("resolution_hash", &request.resolution_hash)?;
    if request.outcome == AppealStatus::Open {
        return Err(ApiError::bad_request(
            "invalid_appeal_outcome",
            "appeal resolution must be upheld or denied",
        ));
    }
    state
        .transact(|league| {
            let appeal = league.appeal_cases.get_mut(&appeal_id).ok_or_else(|| {
                ApiError::not_found(
                    "appeal_not_found",
                    format!("appeal {appeal_id} does not exist"),
                )
            })?;
            if appeal.status != AppealStatus::Open {
                return Err(ApiError::conflict(
                    "appeal_already_resolved",
                    "appeal is immutable after resolution",
                ));
            }
            appeal.status = request.outcome;
            appeal.resolution_hash = Some(request.resolution_hash);
            appeal.adjusted_score_micros = request.adjusted_score_micros;
            appeal.resolved_at = Some(Utc::now());
            let appeal = appeal.clone();
            if appeal.status == AppealStatus::Upheld {
                if let Some(evaluation) = league
                    .evaluation_reports
                    .get_mut(&appeal.evaluation_report_id)
                {
                    evaluation.status = EvaluationStatus::SupersededOnAppeal;
                }
            }
            push_event(
                league,
                "hepta.appeal.resolved.v1",
                appeal.appeal_id.to_string(),
                serde_json::to_value(&appeal).expect("serialize resolved appeal"),
            );
            Ok(Json(appeal))
        })
        .await
}

async fn create_evaluation_commitment(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(
        state,
        headers,
        TrnmCommandKind::EvaluationCommitment,
        request,
    )
    .await
}

async fn create_workload_receipt(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(state, headers, TrnmCommandKind::WorkloadReceipt, request).await
}

async fn create_research_claim(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(state, headers, TrnmCommandKind::ResearchClaim, request).await
}

async fn create_license_declaration(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(state, headers, TrnmCommandKind::LicenseDeclaration, request).await
}

async fn create_claim_challenge(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(state, headers, TrnmCommandKind::ClaimChallenge, request).await
}

async fn create_challenge_resolution(
    state: State<AppState>,
    headers: HeaderMap,
    request: Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    create_trnm_command(state, headers, TrnmCommandKind::ClaimResolution, request).await
}

async fn create_trnm_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    kind: TrnmCommandKind,
    Json(request): Json<CreateTrnmCommandRequest>,
) -> Result<(StatusCode, Json<TrnmCommand>), ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    request.signed_command.validate().map_err(|error| {
        ApiError::bad_request(
            "invalid_trnm_signed_command",
            format!("TRNM research protocol command rejected: {error}"),
        )
    })?;
    if request.signed_command.signer_role != AuthorityRole::HeptaAuthority {
        return Err(ApiError::bad_request(
            "invalid_trnm_command_authority",
            "Hepta endpoints accept only hepta_authority signed commands",
        ));
    }
    if command_kind(&request.signed_command.command).as_ref() != Some(&kind) {
        return Err(ApiError::bad_request(
            "trnm_command_kind_mismatch",
            "typed command does not match the requested TRNM endpoint",
        ));
    }
    let aggregate_id = request
        .signed_command
        .command
        .primary_object_ref()
        .key
        .to_hex();
    let idempotency_key = request.signed_command.command_id.to_hex();
    let command_fingerprint = format_digest(&request.signed_command.command_fingerprint());
    let paper_binding_fingerprint = request
        .paper_binding
        .as_ref()
        .map(crate::paper_chain_finality_v1::paper_binding_fingerprint)
        .transpose()?;
    if let Some(binding) = &request.paper_binding {
        crate::paper_chain_finality_v1::validate_signed_paper_binding(
            &request.signed_command,
            binding,
        )?;
    }
    if let Some(existing) = state
        .inspect(|league| {
            Ok(league
                .trnm_commands
                .values()
                .find(|command| command.idempotency_key == idempotency_key)
                .cloned())
        })
        .await?
    {
        if existing.kind == kind
            && existing.aggregate_id == aggregate_id
            && existing.command_fingerprint == command_fingerprint
            && existing.paper_binding == request.paper_binding
            && existing.paper_binding_fingerprint == paper_binding_fingerprint
        {
            return Ok((StatusCode::OK, Json(existing)));
        }
        return Err(ApiError::conflict(
            "trnm_idempotency_conflict",
            "TRNM idempotency key was reused with different command or Paper binding data",
        ));
    }
    if let Some(binding) = &request.paper_binding {
        crate::paper_chain_finality_v1::validate_paper_binding_for_queue(&state, binding).await?;
    }
    state
        .transact(|league| {
            if let Some(existing) = league
                .trnm_commands
                .values()
                .find(|command| command.idempotency_key == idempotency_key)
            {
                if existing.kind == kind
                    && existing.aggregate_id == aggregate_id
                    && existing.command_fingerprint == command_fingerprint
                    && existing.paper_binding == request.paper_binding
                    && existing.paper_binding_fingerprint == paper_binding_fingerprint
                {
                    return Ok((StatusCode::OK, Json(existing.clone())));
                }
                return Err(ApiError::conflict(
                    "trnm_idempotency_conflict",
                    "TRNM idempotency key was reused with different command or Paper binding data",
                ));
            }
            let command = TrnmCommand {
                command_id: Uuid::new_v4(),
                kind,
                aggregate_id,
                idempotency_key,
                command_fingerprint,
                paper_binding: request.paper_binding,
                paper_binding_fingerprint,
                signed_command: request.signed_command,
                status: TrnmProjectionStatus::PendingFinality,
                created_at: Utc::now(),
            };
            league
                .trnm_commands
                .insert(command.command_id, command.clone());
            push_event(
                league,
                "hepta.trnm.command.requested.v2",
                command.aggregate_id.clone(),
                serde_json::to_value(&command).expect("serialize TRNM command"),
            );
            Ok((StatusCode::ACCEPTED, Json(command)))
        })
        .await
}

async fn ingest_trnm_finality(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(receipt): Json<FinalityReceiptV1>,
) -> Result<(StatusCode, Json<TrnmFinalityProjection>), ApiError> {
    require_service_token(
        &headers,
        TRNM_TOKEN_HEADER,
        &state.security.trnm_token,
        "trnm_auth_failed",
    )?;
    let command = state
        .inspect(|league| {
            league
                .trnm_commands
                .get(&receipt.command_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_command_not_found",
                        format!("TRNM command {} does not exist", receipt.command_id),
                    )
                })
        })
        .await?;
    reject_legacy_finality_for_paper_command(&command)?;
    require_verified_finality_mode(&state)?;
    let verified = verify_finality_receipt(
        &receipt,
        Some(&command.command_fingerprint),
        &state.security.trusted_trnm_validator_sets,
    )
    .map_err(|error| {
        ApiError::bad_request(
            "trnm_finality_verification_failed",
            format!("finality receipt rejected: {error}"),
        )
    })?;
    state
        .transact(|league| {
            let command = league
                .trnm_commands
                .get(&receipt.command_id)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_command_not_found",
                        format!("TRNM command {} does not exist", receipt.command_id),
                    )
                })?;
            reject_legacy_finality_for_paper_command(command)?;
            let inbox_key = format!("trnm-finality-v2:{}", receipt.source_event_id);
            if let Some(existing_hash) = league.inbox_events.get(&inbox_key) {
                if existing_hash != &receipt.receipt_hash {
                    return Err(ApiError::conflict(
                        "inbox_event_conflict",
                        "TRNM event id was reused with a different receipt hash",
                    ));
                }
                let existing = league
                    .trnm_finality
                    .get(&receipt.command_id)
                    .ok_or_else(|| {
                        ApiError::internal("TRNM inbox receipt exists without projection")
                    })?;
                return Ok((StatusCode::OK, Json(existing.clone())));
            }
            let command = league
                .trnm_commands
                .get_mut(&receipt.command_id)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_command_not_found",
                        format!("TRNM command {} does not exist", receipt.command_id),
                    )
                })?;
            if command.command_fingerprint != receipt.command_fingerprint {
                return Err(ApiError::conflict(
                    "trnm_command_fingerprint_conflict",
                    "verified receipt is not bound to the queued command fingerprint",
                ));
            }
            let status = TrnmProjectionStatus::Finalized;
            command.status = status.clone();
            let projection = TrnmFinalityProjection {
                command_id: receipt.command_id,
                source_event_id: receipt.source_event_id,
                chain_id: receipt.chain_id,
                tx_hash: receipt.tx_hash,
                tx_index: receipt.tx_index,
                block_height: receipt.block_height,
                block_hash: receipt.block_hash,
                state_root: receipt.state_root,
                object_ref: receipt.object_ref,
                inclusion_proof: receipt.inclusion_proof,
                validator_set_id: receipt.validator_set_id,
                quorum_certificate: receipt.quorum_certificate,
                confirmations: receipt.confirmations,
                status,
                receipt_hash: receipt.receipt_hash.clone(),
                verified: verified.valid,
                updated_at: Utc::now(),
            };
            league.inbox_events.insert(inbox_key, receipt.receipt_hash);
            league
                .trnm_finality
                .insert(projection.command_id, projection.clone());
            push_event(
                league,
                "trnm.receipt.projected.v2",
                projection.command_id.to_string(),
                serde_json::to_value(&projection).expect("serialize finality projection"),
            );
            Ok((StatusCode::ACCEPTED, Json(projection)))
        })
        .await
}

async fn ingest_live_trnm_finality(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LiveTrnmFinalityRequestV1>,
) -> Result<(StatusCode, Json<LiveTrnmFinalityProjection>), ApiError> {
    require_service_token(
        &headers,
        TRNM_TOKEN_HEADER,
        &state.security.trnm_token,
        "trnm_auth_failed",
    )?;
    let command_id = Uuid::parse_str(&request.receipt.command_id).map_err(|_| {
        ApiError::bad_request(
            "invalid_live_trnm_command_id",
            "live Chain receipt command_id must be the queued Hepta UUID",
        )
    })?;
    let command = state
        .inspect(|league| {
            league
                .trnm_commands
                .get(&command_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_command_not_found",
                        format!("TRNM command {command_id} does not exist"),
                    )
                })
        })
        .await?;
    reject_legacy_finality_for_paper_command(&command)?;
    require_verified_finality_mode(&state)?;
    verify_live_receipt_binding(
        &request.receipt,
        &command,
        &state.security.trusted_trnm_validator_sets,
    )
    .map_err(|error| {
        ApiError::bad_request(
            "trnm_live_finality_verification_failed",
            format!("live Chain receipt rejected: {error}"),
        )
    })?;

    state
        .transact(|league| {
            let queued = league
                .trnm_commands
                .get(&command_id)
                .ok_or_else(|| ApiError::internal("queued TRNM command disappeared"))?;
            reject_legacy_finality_for_paper_command(queued)?;
            let inbox_key = format!("trnm-live-finality-v1:{}", request.source_event_id);
            if let Some(existing_hash) = league.inbox_events.get(&inbox_key) {
                if existing_hash != &request.receipt.receipt_hash_hex {
                    return Err(ApiError::conflict(
                        "inbox_event_conflict",
                        "TRNM live event id was reused with a different receipt hash",
                    ));
                }
                let existing = league.trnm_live_finality.get(&command_id).ok_or_else(|| {
                    ApiError::internal("TRNM live inbox receipt exists without projection")
                })?;
                return Ok((StatusCode::OK, Json(existing.clone())));
            }
            let queued = league
                .trnm_commands
                .get_mut(&command_id)
                .ok_or_else(|| ApiError::internal("queued TRNM command disappeared"))?;
            queued.status = TrnmProjectionStatus::Finalized;
            let projection = LiveTrnmFinalityProjection {
                command_id,
                source_event_id: request.source_event_id,
                command_fingerprint: command.command_fingerprint,
                receipt: request.receipt,
                status: TrnmProjectionStatus::Finalized,
                verified: true,
                updated_at: Utc::now(),
            };
            league
                .inbox_events
                .insert(inbox_key, projection.receipt.receipt_hash_hex.clone());
            league
                .trnm_live_finality
                .insert(command_id, projection.clone());
            push_event(
                league,
                "trnm.live.receipt.projected.v1",
                command_id.to_string(),
                serde_json::to_value(&projection).expect("serialize live finality projection"),
            );
            Ok((StatusCode::ACCEPTED, Json(projection)))
        })
        .await
}

fn reject_legacy_finality_for_paper_command(command: &TrnmCommand) -> Result<(), ApiError> {
    if command.paper_binding.is_some() {
        return Err(ApiError::conflict(
            "paper_trnm_legacy_finality_forbidden",
            "Paper-bound TRNM commands can be finalized only by the Receipt V2 Paper finality endpoint",
        ));
    }
    Ok(())
}

async fn get_live_trnm_finality(
    State(state): State<AppState>,
    Path(command_id): Path<Uuid>,
) -> Result<Json<LiveTrnmFinalityProjection>, ApiError> {
    state
        .inspect(|league| {
            league
                .trnm_live_finality
                .get(&command_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_live_finality_not_found",
                        format!("live TRNM finality for command {command_id} does not exist"),
                    )
                })
        })
        .await
}

fn verify_live_receipt_binding(
    receipt: &LiveFinalityReceiptV1,
    command: &TrnmCommand,
    trusted_sets: &[crate::trnm_v1::TrustedValidatorSetV1],
) -> Result<(), String> {
    let expected_fingerprint = command
        .command_fingerprint
        .strip_prefix("sha256:")
        .ok_or_else(|| "queued command fingerprint is not canonical".to_string())?;
    if receipt.domain_command_fingerprint_hex.as_deref() != Some(expected_fingerprint) {
        return Err("live receipt domain command fingerprint does not match queued command".into());
    }
    let primary_ref = command.signed_command.command.primary_object_ref();
    let object_ref = receipt
        .object_ref
        .as_ref()
        .ok_or_else(|| "live receipt is missing its research object reference".to_string())?;
    let mut expected_key = Vec::with_capacity(33);
    expected_key.push(primary_ref.kind as u8);
    expected_key.extend_from_slice(primary_ref.key.as_bytes());
    if object_ref.object_key_hex != hex_lower(&expected_key)
        || object_ref.object_type != live_research_object_type(primary_ref.kind)
        || object_ref.version != primary_ref.object_version
    {
        return Err("live receipt object reference does not match queued research command".into());
    }
    let trusted = trusted_sets
        .iter()
        .find(|set| {
            set.chain_id == receipt.chain_id && set.validator_set_id == receipt.validator_set_id
        })
        .ok_or_else(|| "live receipt validator set is not locally trusted".to_string())?;
    let validator_set = live_validator_set(trusted)?;
    verify_live_finality_receipt(receipt, &validator_set)
        .map_err(|error| format!("verify live Chain receipt: {error}"))
}

fn live_validator_set(
    trusted: &crate::trnm_v1::TrustedValidatorSetV1,
) -> Result<LiveValidatorSetV1, String> {
    trusted.validate()?;
    let mut total_power = 0_u64;
    let mut validators = Vec::with_capacity(trusted.validators.len());
    for (index, validator) in trusted.validators.iter().enumerate() {
        let key = BASE64
            .decode(&validator.public_key_base64)
            .map_err(|error| format!("decode trusted validator public key: {error}"))?;
        if key.len() != 32 {
            return Err("trusted validator public key must contain 32 bytes".into());
        }
        total_power = total_power
            .checked_add(validator.voting_power)
            .ok_or_else(|| "trusted validator voting power overflow".to_string())?;
        validators.push(ValidatorDescriptorV1 {
            validator_id: validator.validator_id.clone(),
            public_key_hex: hex_lower(&key),
            vote_endpoint: format!("http://127.0.0.1:{}/v1/vote", 30_000 + index),
            voting_power: validator.voting_power,
        });
    }
    Ok(LiveValidatorSetV1 {
        validator_set_id: trusted.validator_set_id.clone(),
        validators,
        quorum_power: total_power
            .checked_mul(2)
            .ok_or_else(|| "trusted validator quorum overflow".to_string())?
            / 3
            + 1,
    })
}

fn live_research_object_type(kind: trnm_research_protocol::ResearchObjectKind) -> &'static str {
    use trnm_research_protocol::ResearchObjectKind;
    match kind {
        ResearchObjectKind::MatchEvidence => "trnm_match_evidence_v1",
        ResearchObjectKind::EvaluationCommitment => "trnm_evaluation_commitment_v1",
        ResearchObjectKind::WorkloadReceipt => "trnm_workload_receipt_v1",
        ResearchObjectKind::ResearchClaim => "trnm_research_claim_v1",
        ResearchObjectKind::LicenseDeclaration => "trnm_license_declaration_v1",
        ResearchObjectKind::ClaimChallenge => "trnm_claim_challenge_v1",
        ResearchObjectKind::ClaimResolution => "trnm_claim_resolution_v1",
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

async fn verify_trnm_finality_offline(
    State(state): State<AppState>,
    Json(receipt): Json<FinalityReceiptV1>,
) -> Result<Json<VerifiedFinalityV1>, ApiError> {
    require_verified_finality_mode(&state)?;
    verify_finality_receipt(&receipt, None, &state.security.trusted_trnm_validator_sets)
        .map(Json)
        .map_err(|error| {
            ApiError::bad_request(
                "trnm_finality_verification_failed",
                format!("finality receipt rejected: {error}"),
            )
        })
}

async fn get_trnm_finality(
    State(state): State<AppState>,
    Path(command_id): Path<Uuid>,
) -> Result<Json<TrnmFinalityProjection>, ApiError> {
    state
        .inspect(|league| {
            league
                .trnm_finality
                .get(&command_id)
                .cloned()
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "trnm_finality_not_found",
                        format!("TRNM command {command_id} is still pending finality"),
                    )
                })
        })
        .await
}

async fn ingest_nakama_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<NakamaMatchEvent>,
) -> Result<(StatusCode, Json<NakamaMatchProjection>), ApiError> {
    require_service_token(
        &headers,
        NAKAMA_TOKEN_HEADER,
        &state.security.nakama_token,
        "nakama_auth_failed",
    )?;
    if event.protocol != NAKAMA_EVENT_V2 {
        return Err(ApiError::bad_request(
            "unsupported_nakama_protocol",
            format!("expected {NAKAMA_EVENT_V2}"),
        ));
    }
    validate_hash("event_hash", &event.event_hash)?;
    if let Some(root) = &event.event_root {
        validate_hash("event_root", root)?;
    }
    state
        .transact(|league| {
            let inbox_key = format!("nakama-match-events-v2:{}", event.event_id);
            if let Some(existing_hash) = league.inbox_events.get(&inbox_key) {
                if existing_hash != &event.event_hash {
                    return Err(ApiError::conflict(
                        "inbox_event_conflict",
                        "Nakama event id was reused with a different event hash",
                    ));
                }
                let existing = league.nakama_matches.get(&event.match_id).ok_or_else(|| {
                    ApiError::internal("Nakama inbox receipt exists without match projection")
                })?;
                return Ok((StatusCode::OK, Json(existing.clone())));
            }
            if !league.challenges.contains_key(&event.challenge_id) {
                return Err(ApiError::not_found(
                    "challenge_not_found",
                    format!("challenge {} does not exist", event.challenge_id),
                ));
            }
            let projection =
                league
                    .nakama_matches
                    .entry(event.match_id)
                    .or_insert(NakamaMatchProjection {
                        match_id: event.match_id,
                        challenge_id: event.challenge_id,
                        status: NakamaMatchStatus::Started,
                        last_sequence: 0,
                        event_hashes: Vec::new(),
                        event_root: None,
                        archive_uri: None,
                        updated_at: event.occurred_at,
                    });
            if projection.challenge_id != event.challenge_id {
                return Err(ApiError::conflict(
                    "nakama_challenge_mismatch",
                    "Nakama match cannot change its Hepta challenge snapshot",
                ));
            }
            if event.sequence != projection.last_sequence + 1 {
                return Err(ApiError::conflict(
                    "nakama_sequence_gap",
                    format!(
                        "expected sequence {}, received {}",
                        projection.last_sequence + 1,
                        event.sequence
                    ),
                ));
            }
            match event.event_type.as_str() {
                "nakama.match.started.v1" if event.sequence == 1 => {
                    projection.status = NakamaMatchStatus::Started;
                }
                "nakama.round.closed.v1" if projection.status != NakamaMatchStatus::Completed => {
                    projection.status = NakamaMatchStatus::InProgress;
                }
                "nakama.match.completed.v1"
                    if projection.status != NakamaMatchStatus::Completed =>
                {
                    let supplied_root = event.event_root.clone().ok_or_else(|| {
                        ApiError::bad_request(
                            "nakama_event_root_required",
                            "completed match must include event_root",
                        )
                    })?;
                    let mut hashes = projection.event_hashes.clone();
                    hashes.push(event.event_hash.clone());
                    let computed = match_event_merkle_root(&hashes)?;
                    if supplied_root != computed {
                        return Err(ApiError::conflict(
                            "nakama_event_root_mismatch",
                            format!("supplied event root does not match computed root {computed}"),
                        ));
                    }
                    projection.status = NakamaMatchStatus::Completed;
                    projection.event_root = Some(supplied_root);
                    projection.archive_uri = event.archive_uri.clone();
                }
                _ => {
                    return Err(ApiError::conflict(
                        "invalid_nakama_transition",
                        "Nakama event type is invalid for the current authoritative match state",
                    ));
                }
            }
            projection.last_sequence = event.sequence;
            projection.event_hashes.push(event.event_hash.clone());
            projection.updated_at = event.occurred_at;
            let projection = projection.clone();
            league.inbox_events.insert(inbox_key, event.event_hash);
            push_event(
                league,
                &event.event_type,
                event.match_id.to_string(),
                serde_json::to_value(&projection).expect("serialize Nakama projection"),
            );
            if projection.status == NakamaMatchStatus::Completed {
                push_event(
                    league,
                    "hepta.nakama.event_root.accepted.v2",
                    event.match_id.to_string(),
                    json!({
                        "match_id": event.match_id,
                        "challenge_id": event.challenge_id,
                        "event_root": projection.event_root,
                        "archive_uri": projection.archive_uri,
                    }),
                );
            }
            Ok((StatusCode::ACCEPTED, Json(projection)))
        })
        .await
}

async fn reconcile_nakama_match(
    State(state): State<AppState>,
    Path(match_id): Path<Uuid>,
) -> Result<Json<ReconciliationView>, ApiError> {
    state
        .inspect(|league| {
            let projection = league.nakama_matches.get(&match_id).ok_or_else(|| {
                ApiError::not_found(
                    "nakama_match_not_found",
                    format!("Nakama match {match_id} does not exist"),
                )
            })?;
            let computed_event_root = match_event_merkle_root(&projection.event_hashes)?;
            let reconciled = projection.status != NakamaMatchStatus::Completed
                || projection.event_root.as_deref() == Some(computed_event_root.as_str());
            Ok(Json(ReconciliationView {
                match_id,
                status: projection.status.clone(),
                sequence_contiguous: projection.last_sequence
                    == projection.event_hashes.len() as u64,
                event_root: projection.event_root.clone(),
                computed_event_root,
                reconciled,
            }))
        })
        .await
}

async fn task_package(
    State(state): State<AppState>,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .inspect(|league| {
            let challenge = league.challenges.get(&challenge_id).ok_or_else(|| {
                ApiError::not_found(
                    "challenge_not_found",
                    format!("challenge {challenge_id} does not exist"),
                )
            })?;
            if challenge.status != ChallengeStatus::Open {
                return Err(ApiError::conflict(
                    "challenge_not_open",
                    "task packages are only issued for open challenges",
                ));
            }
            let evaluator = league
                .evaluator_manifests
                .values()
                .filter(|manifest| manifest.challenge_id == challenge_id)
                .max_by_key(|manifest| &manifest.created_at);
            Ok(Json(json!({
                "protocol": "hepta_research_task_package_v1",
                "agent_execution_mode": "external_only",
                "challenge": challenge,
                "evaluator_manifest": evaluator,
                "platform_model_credentials": null,
            })))
        })
        .await
}

async fn agent_profile(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .inspect(|league| {
            let agent = league.agents.get(&agent_id).ok_or_else(|| {
                ApiError::not_found(
                    "agent_not_found",
                    format!("agent {agent_id} does not exist"),
                )
            })?;
            let submissions = league
                .submissions
                .values()
                .filter(|submission| submission.agent_id == agent_id)
                .collect::<Vec<_>>();
            Ok(Json(json!({
                "agent": agent,
                "submissions": submissions,
                "execution_mode": "external_only",
            })))
        })
        .await
}

async fn operator_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EventEnvelope>>, ApiError> {
    require_service_token(
        &headers,
        OPERATOR_TOKEN_HEADER,
        &state.security.operator_token,
        "operator_auth_failed",
    )?;
    state
        .inspect(|league| Ok(Json(league.events.clone())))
        .await
}

fn validate_criteria(criteria: &[EvaluationCriterion]) -> Result<(), ApiError> {
    if criteria.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_evaluator_manifest",
            "at least one evaluation criterion is required",
        ));
    }
    let mut metrics = HashSet::new();
    let mut total_weight = 0_u32;
    for criterion in criteria {
        validate_non_empty("criterion.metric", &criterion.metric)?;
        if !metrics.insert(criterion.metric.clone()) {
            return Err(ApiError::bad_request(
                "duplicate_evaluation_metric",
                format!("metric {} appears more than once", criterion.metric),
            ));
        }
        total_weight += u32::from(criterion.weight_bps);
    }
    if total_weight != 10_000 {
        return Err(ApiError::bad_request(
            "invalid_evaluation_weights",
            format!("evaluation criterion weights must total 10000 bps, got {total_weight}"),
        ));
    }
    Ok(())
}

fn deterministic_score(
    manifest: &EvaluatorManifest,
    metrics: &BTreeMap<String, i64>,
) -> Result<(i64, bool), ApiError> {
    if metrics.len() != manifest.criteria.len()
        || manifest
            .criteria
            .iter()
            .any(|criterion| !metrics.contains_key(&criterion.metric))
    {
        return Err(ApiError::bad_request(
            "evaluation_metric_set_mismatch",
            "observed metrics must exactly match the versioned evaluator manifest",
        ));
    }
    let mut weighted: i128 = 0;
    let mut constraints_satisfied = true;
    for criterion in &manifest.criteria {
        let value = metrics[&criterion.metric];
        weighted = weighted
            .checked_add(i128::from(value) * i128::from(criterion.weight_bps))
            .ok_or_else(|| ApiError::bad_request("score_overflow", "evaluation score overflow"))?;
        if criterion
            .minimum_micros
            .is_some_and(|minimum| value < minimum)
        {
            constraints_satisfied = false;
        }
    }
    let score = weighted / 10_000;
    let score = i64::try_from(score)
        .map_err(|_| ApiError::bad_request("score_overflow", "evaluation score overflow"))?;
    Ok((score, constraints_satisfied))
}

fn hash_value(value: &Value) -> String {
    format!(
        "sha256:{}",
        sha256_hex(&serde_json::to_vec(value).expect("serialize canonical JSON"))
    )
}

fn match_event_merkle_root(event_hashes: &[String]) -> Result<String, ApiError> {
    if event_hashes.is_empty() {
        return Ok(format!(
            "sha256:{}",
            sha256_hex(b"trnm_match_event_merkle_empty_v1")
        ));
    }
    let mut level = event_hashes
        .iter()
        .enumerate()
        .map(|(index, event_hash)| {
            let event_hash = decode_sha256(event_hash).map_err(|error| {
                ApiError::bad_request(
                    "invalid_nakama_event_hash",
                    format!("event hash {} is invalid: {error}", index + 1),
                )
            })?;
            let mut hasher = Sha256::new();
            hasher.update(b"trnm_match_event_leaf_v1\0");
            hasher.update(((index + 1) as u64).to_be_bytes());
            hasher.update(event_hash);
            Ok::<[u8; 32], ApiError>(hasher.finalize().into())
        })
        .collect::<Result<Vec<_>, _>>()?;
    while level.len() > 1 {
        let mut parents = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = pair[0];
            let right = pair.get(1).copied().unwrap_or(left);
            let mut hasher = Sha256::new();
            hasher.update(b"trnm_binary_merkle_node_v1\0");
            hasher.update(left);
            hasher.update(right);
            parents.push(hasher.finalize().into());
        }
        level = parents;
    }
    Ok(format!("sha256:{}", hex_bytes(&level[0])))
}

fn decode_sha256(value: &str) -> Result<[u8; 32], String> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| "expected sha256:<64 lowercase hex>".to_string())?;
    if hex.len() != 64
        || !hex
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err("expected sha256:<64 lowercase hex>".to_string());
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid hash hex".to_string())?;
    }
    Ok(bytes)
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use sqlx::postgres::PgPoolOptions;
    use std::time::Duration;

    #[test]
    fn deterministic_scoring_is_fixed_point_and_order_independent() {
        let manifest = EvaluatorManifest {
            evaluator_manifest_id: Uuid::nil(),
            challenge_id: Uuid::nil(),
            version: "v1".into(),
            protocol: EVALUATOR_MANIFEST_V1.into(),
            criteria: vec![
                EvaluationCriterion {
                    metric: "quality".into(),
                    weight_bps: 7_500,
                    minimum_micros: Some(500_000),
                },
                EvaluationCriterion {
                    metric: "efficiency".into(),
                    weight_bps: 2_500,
                    minimum_micros: None,
                },
            ],
            manifest_hash: "unused".into(),
            created_at: Utc::now(),
        };
        let metrics = BTreeMap::from([("efficiency".into(), 800_000), ("quality".into(), 600_000)]);
        assert_eq!(
            deterministic_score(&manifest, &metrics).unwrap(),
            (650_000, true)
        );
    }

    #[test]
    fn match_event_root_changes_when_any_leaf_changes() {
        let original = match_event_merkle_root(&[
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        ])
        .unwrap();
        let tampered = match_event_merkle_root(&[
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into(),
        ])
        .unwrap();
        assert_ne!(original, tampered);
    }

    #[tokio::test]
    async fn readiness_fails_closed_without_durable_storage_or_nakama_trust() {
        let state = AppState::new(crate::SecurityConfig::new("operator", "nakama"));
        let (status, Json(response)) = ready(State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.ready);
        assert_eq!(response.database, "not_configured");
        assert_eq!(response.security, "invalid");
        assert!(response.failures.contains(&"database_pool_missing"));
        assert!(response
            .failures
            .contains(&"nakama_research_authority_missing"));
    }

    #[tokio::test]
    async fn readiness_fails_closed_when_verified_mode_has_no_receipt_v2_pins() {
        let security = crate::SecurityConfig::new("operator", "nakama")
            .with_trnm_token("trnm")
            .with_finality_mode(FinalityMode::Verified);
        assert_eq!(
            security
                .validate_finality_startup()
                .expect_err("verified startup without Receipt V2 pins must fail closed"),
            "verified HEPTA_FINALITY_MODE requires HEPTA_TRNM_COMETBFT_TRUST_ANCHOR_HASHES_JSON with at least one pinned trust-anchor hash"
        );
        let pinned_only = crate::SecurityConfig::new("operator", "nakama")
            .with_trnm_token("trnm")
            .with_finality_mode(FinalityMode::Verified)
            .with_pinned_trnm_cometbft_trust_anchor_hash("11".repeat(32))
            .expect("valid Receipt V2 pin");
        pinned_only
            .validate_finality_startup()
            .expect("pinned-only verified startup is valid");
        let state = AppState::new(security);
        let (status, Json(response)) = ready(State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.ready);
        assert_eq!(response.finality_mode, "verified");
        assert_eq!(response.pinned_cometbft_trust_anchor_hashes, 0);
        assert_eq!(
            response.trnm_receipt_v2_max_body_bytes,
            crate::DEFAULT_TRNM_RECEIPT_V2_MAX_BODY_BYTES
        );
        assert_eq!(
            response.trnm_receipt_v2_max_in_flight,
            crate::DEFAULT_TRNM_RECEIPT_V2_MAX_IN_FLIGHT
        );
        assert!(response.failures.contains(&"trnm_finality_trust_missing"));
    }

    #[tokio::test]
    async fn readiness_fails_closed_when_postgres_pool_cannot_connect() {
        let security = crate::SecurityConfig::new("operator", "nakama")
            .with_trnm_token("trnm")
            .with_trusted_nakama_research_authority(
                "nakama-readiness-unit-v1",
                SigningKey::from_bytes(&[0x71; 32])
                    .verifying_key()
                    .to_bytes(),
            )
            .expect("valid Nakama authority");
        let mut state = AppState::new(security);
        state.pool = Some(
            PgPoolOptions::new()
                .acquire_timeout(Duration::from_millis(100))
                .connect_lazy("postgres://127.0.0.1:1/hepta_unreachable")
                .expect("lazy pool"),
        );
        let (status, Json(response)) = ready(State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!response.ready);
        assert_eq!(response.database, "unreachable");
        assert_eq!(response.security, "valid");
        assert!(response
            .failures
            .iter()
            .any(|failure| failure.starts_with("database_")));
    }
}
