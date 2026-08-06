use std::collections::{BTreeMap, HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use super::collaboration_v3::{insert_room_event_postgres, push_room_event_memory};
use super::*;
use crate::paper_raid_contracts::{
    canonical_json_sha256, paper_appeal_resolution_signing_bytes, paper_appeal_signing_bytes,
    paper_evaluation_signing_bytes, paper_reproduction_signing_bytes,
    paper_review_attestation_signing_bytes, sha256_digest, PaperAppealResolutionSigningV1,
    PaperAppealSigningV1, PaperEvaluationSigningV1, PaperReproductionSigningV1,
    PaperReviewAttestationSigningV1, PAPER_APPEAL_RESOLUTION_V1, PAPER_APPEAL_V1,
    PAPER_EVALUATION_V1, PAPER_REPRODUCTION_V1, PAPER_REVIEW_ATTESTATION_V1,
};

pub const PAPER_REVIEW_PROTOCOL_V4: &str = "hepta.paper_raid.review.v4";
pub const CONTRIBUTION_LEDGER_SCHEMA_V1: &str = "hepta.paper_raid.contribution_ledger.v1";
pub const PAPER_SCORE_SCHEMA_V1: &str = "hepta.paper_raid.paper_score.v1";
pub const RAID_SCORE_SCHEMA_V1: &str = "hepta.paper_raid.raid_score.v1";
pub const TOLERANCE_POLICY_SCHEMA_V1: &str = "hepta.paper_raid.tolerance_policy.v1";

#[derive(Clone, Default)]
pub(crate) struct ReviewMemory {
    contribution_ledgers: HashMap<Uuid, ContributionLedger>,
    pub(crate) evaluations: HashMap<Uuid, PaperEvaluation>,
    pub(crate) reproductions: HashMap<Uuid, PaperReproduction>,
    pub(crate) appeals: HashMap<Uuid, PaperAppeal>,
    pub(crate) resolutions: HashMap<Uuid, PaperAppealResolution>,
    raid_scores: HashMap<Uuid, RaidScore>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreditContributionInput {
    pub player_id: Uuid,
    pub credit_roles: Vec<String>,
    pub accepted_artifact_manifest_ids: Vec<Uuid>,
    pub accepted_section_review_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreditContribution {
    pub player_id: Uuid,
    pub credit_roles: Vec<String>,
    pub accepted_artifact_manifest_ids: Vec<Uuid>,
    pub accepted_section_review_ids: Vec<Uuid>,
    pub contribution_points: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContributionLedger {
    pub schema: String,
    pub contribution_ledger_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub entries: Vec<CreditContribution>,
    pub ledger_hash: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateContributionLedgerRequest {
    pub contribution_ledger_id: Uuid,
    pub expected_paper_version: u64,
    pub release_candidate_hash: String,
    pub entries: Vec<CreditContributionInput>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperScoreComponents {
    pub method_rigor_bps: u16,
    pub experiment_statistics_bps: u16,
    pub reproducibility_bps: u16,
    pub evidence_citations_bps: u16,
    pub value_originality_bps: u16,
    pub argument_expression_bps: u16,
    pub ethics_transparency_bps: u16,
}

impl PaperScoreComponents {
    fn validate_and_total(&self) -> Result<u16, ApiError> {
        for (field, actual, maximum) in [
            ("method_rigor_bps", self.method_rigor_bps, 2_500),
            (
                "experiment_statistics_bps",
                self.experiment_statistics_bps,
                1_500,
            ),
            ("reproducibility_bps", self.reproducibility_bps, 1_500),
            ("evidence_citations_bps", self.evidence_citations_bps, 1_500),
            ("value_originality_bps", self.value_originality_bps, 1_500),
            (
                "argument_expression_bps",
                self.argument_expression_bps,
                1_000,
            ),
            ("ethics_transparency_bps", self.ethics_transparency_bps, 500),
        ] {
            if actual > maximum {
                return Err(ApiError::bad_request(
                    "paper_score_component_out_of_range",
                    format!("{field} exceeds its frozen {maximum} bps maximum"),
                ));
            }
        }
        Ok(self.method_rigor_bps
            + self.experiment_statistics_bps
            + self.reproducibility_bps
            + self.evidence_citations_bps
            + self.value_originality_bps
            + self.argument_expression_bps
            + self.ethics_transparency_bps)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperHardGates {
    pub citations_and_data_authentic: bool,
    pub failed_runs_disclosed: bool,
    pub all_authors_consented: bool,
    pub core_claims_have_evidence: bool,
    pub artifact_lineage_complete: bool,
    pub license_ethics_coi_complete: bool,
}

impl PaperHardGates {
    fn eligible(&self) -> bool {
        self.citations_and_data_authentic
            && self.failed_runs_disclosed
            && self.all_authors_consented
            && self.core_claims_have_evidence
            && self.artifact_lineage_complete
            && self.license_ethics_coi_complete
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ToleranceRule {
    Absolute {
        metric: String,
        max_delta_micros: u64,
    },
    Relative {
        metric: String,
        max_delta_bps: u16,
    },
    Statistical {
        metric: String,
        minimum_interval_overlap_bps: u16,
        maximum_effect_delta_micros: u64,
        minimum_p_value_micros: u32,
    },
    Seed {
        expected_seed_set_hash: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TolerancePolicy {
    pub schema: String,
    pub version: String,
    pub rules: Vec<ToleranceRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StatisticalEvidence {
    pub interval_overlap_bps: u16,
    pub effect_delta_micros: i64,
    pub p_value_micros: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PanelVerdict {
    Approve,
    Reject,
}

impl PanelVerdict {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Reject => "reject",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewAttestationRequest {
    pub attestation_id: Uuid,
    pub reviewer_player_id: Uuid,
    pub verdict: PanelVerdict,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAttestation {
    pub attestation_id: Uuid,
    pub evaluation_id: Uuid,
    pub evaluation_signing_hash: String,
    pub reviewer_player_id: Uuid,
    pub verdict: PanelVerdict,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperEvaluationStatus {
    Accepted,
    Rejected,
    NotEligible,
}

impl PaperEvaluationStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::NotEligible => "not_eligible",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSettlementState {
    PendingFinality,
    Challenged,
    Resolved,
}

impl ReviewSettlementState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::PendingFinality => "pending_finality",
            Self::Challenged => "challenged",
            Self::Resolved => "resolved",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperScore {
    pub schema: String,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub components: PaperScoreComponents,
    pub hard_gates: PaperHardGates,
    pub score_bps: u16,
    pub eligible: bool,
    pub score_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaidScore {
    pub schema: String,
    pub raid_score_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub team_xp: u64,
    pub player_xp: BTreeMap<Uuid, u64>,
    pub paper_score_excluded: bool,
    pub score_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperEvaluation {
    pub schema: String,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub supersedes_evaluation_id: Option<Uuid>,
    pub tolerance_policy: TolerancePolicy,
    pub tolerance_policy_hash: String,
    pub reference_metrics_micros: BTreeMap<String, i64>,
    pub evaluator_player_id: Uuid,
    pub evaluator_signing_key_id: String,
    pub evaluator_signing_public_key: String,
    pub evaluator_signing_public_key_hash: String,
    pub evaluator_coi_attestation_hash: String,
    pub evaluator_signed_at_unix: i64,
    pub evaluator_signature: String,
    pub reviewer_attestations: Vec<ReviewAttestation>,
    pub paper_score: PaperScore,
    pub status: PaperEvaluationStatus,
    pub settlement_state: ReviewSettlementState,
    pub evaluation_signing_hash: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperEvaluationRequest {
    pub evaluation_id: Uuid,
    pub submission_id: Uuid,
    pub supersedes_evaluation_id: Option<Uuid>,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub tolerance_policy: TolerancePolicy,
    pub reference_metrics_micros: BTreeMap<String, i64>,
    pub score_components: PaperScoreComponents,
    pub hard_gates: PaperHardGates,
    pub evaluator_player_id: Uuid,
    pub evaluator_signing_key_id: String,
    pub evaluator_signing_public_key: String,
    pub evaluator_signing_public_key_hash: String,
    pub evaluator_coi_attestation_hash: String,
    pub evaluator_signed_at_unix: i64,
    pub evaluator_signature: String,
    pub reviewer_attestations: Vec<ReviewAttestationRequest>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionStatus {
    Reproduced,
    FailedTolerance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleResult {
    pub rule_key: String,
    pub passed: bool,
    pub detail_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReproduction {
    pub schema: String,
    pub reproduction_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub tolerance_policy_hash: String,
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub statistical_evidence: BTreeMap<String, StatisticalEvidence>,
    pub seed_set_hash: String,
    pub environment_hash: String,
    pub run_manifest_hash: String,
    pub supersedes_reproduction_id: Option<Uuid>,
    pub reproducer_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub rule_results: Vec<RuleResult>,
    pub status: ReproductionStatus,
    pub report_hash: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperReproductionRequest {
    pub reproduction_id: Uuid,
    pub supersedes_reproduction_id: Option<Uuid>,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub observed_metrics_micros: BTreeMap<String, i64>,
    pub statistical_evidence: BTreeMap<String, StatisticalEvidence>,
    pub seed_set_hash: String,
    pub environment_hash: String,
    pub run_manifest_hash: String,
    pub reproducer_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperAppeal {
    pub schema: String,
    pub appeal_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub appellant_player_id: Uuid,
    pub grounds_hash: String,
    pub evidence_manifest_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperAppealRequest {
    pub appeal_id: Uuid,
    pub release_candidate_hash: String,
    pub appellant_player_id: Uuid,
    pub grounds_hash: String,
    pub evidence_manifest_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppealOutcome {
    Upheld,
    Denied,
}

impl AppealOutcome {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Upheld => "upheld",
            Self::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperAppealResolution {
    pub schema: String,
    pub resolution_id: Uuid,
    pub appeal_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub release_candidate_hash: String,
    pub outcome: AppealOutcome,
    pub superseding_evaluation_id: Option<Uuid>,
    pub decision_hash: String,
    pub resolver_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvePaperAppealRequest {
    pub resolution_id: Uuid,
    pub outcome: AppealOutcome,
    pub superseding_evaluation_id: Option<Uuid>,
    pub decision_hash: String,
    pub resolver_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReviewReadModel {
    pub contribution_ledgers: Vec<ContributionLedger>,
    pub evaluations: Vec<PaperEvaluation>,
    pub reproductions: Vec<PaperReproduction>,
    pub appeals: Vec<PaperAppeal>,
    pub resolutions: Vec<PaperAppealResolution>,
    pub raid_scores: Vec<RaidScore>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/v2/hepta/papers/:paper_id/contribution-ledgers",
            post(create_contribution_ledger),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluations",
            post(create_paper_evaluation),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluations/:evaluation_id/reproductions",
            post(create_paper_reproduction),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluations/:evaluation_id/appeals",
            post(create_paper_appeal),
        )
        .route(
            "/v2/hepta/papers/:paper_id/appeals/:appeal_id/resolve",
            post(resolve_paper_appeal),
        )
        .route(
            "/v2/hepta/papers/:paper_id/review-state",
            get(get_paper_review_state),
        )
}

fn signed_time(value: i64) -> Result<DateTime<Utc>, ApiError> {
    let time = Utc.timestamp_opt(value, 0).single().ok_or_else(|| {
        ApiError::bad_request("invalid_signed_at", "signed timestamp is out of range")
    })?;
    let now = Utc::now();
    if time > now + chrono::Duration::minutes(5) || time < now - chrono::Duration::hours(24) {
        return Err(ApiError::forbidden(
            "signed_at_outside_window",
            "signed timestamp is outside the accepted replay window",
        ));
    }
    Ok(time)
}

fn decode_signature(value: &str) -> Result<Signature, ApiError> {
    let bytes = BASE64.decode(value).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "signature must be canonical padded base64",
        )
    })?;
    if BASE64.encode(&bytes) != value {
        return Err(ApiError::bad_request(
            "invalid_signature",
            "signature must be canonical padded base64",
        ));
    }
    Signature::from_slice(&bytes).map_err(|_| {
        ApiError::bad_request("invalid_signature", "signature must decode to 64 bytes")
    })
}

fn validate_digest(field: &'static str, value: &str) -> Result<(), ApiError> {
    validate_digest_v2(field, value)
}

fn record_hash<T: Serialize>(record: &T, kind: &'static str) -> Result<String, ApiError> {
    canonical_json_sha256(record).map_err(|message| {
        ApiError::bad_request("invalid_review_record", format!("{kind}: {message}"))
    })
}

fn validate_tolerance_policy(policy: &TolerancePolicy) -> Result<String, ApiError> {
    if policy.schema != TOLERANCE_POLICY_SCHEMA_V1 {
        return Err(ApiError::bad_request(
            "invalid_tolerance_policy_schema",
            "tolerance policy schema is not supported",
        ));
    }
    validate_contract_text_api("tolerance_policy.version", &policy.version)?;
    if policy.rules.is_empty() || policy.rules.len() > 128 {
        return Err(ApiError::bad_request(
            "invalid_tolerance_policy_rules",
            "tolerance policy requires 1-128 rules",
        ));
    }
    let mut seen = HashSet::new();
    for rule in &policy.rules {
        let key = match rule {
            ToleranceRule::Absolute {
                metric,
                max_delta_micros: _,
            } => {
                validate_contract_text_api("tolerance.metric", metric)?;
                format!("absolute:{metric}")
            }
            ToleranceRule::Relative {
                metric,
                max_delta_bps,
            } => {
                validate_contract_text_api("tolerance.metric", metric)?;
                if *max_delta_bps > 10_000 {
                    return Err(ApiError::bad_request(
                        "invalid_relative_tolerance",
                        "relative max_delta_bps must be <= 10000",
                    ));
                }
                format!("relative:{metric}")
            }
            ToleranceRule::Statistical {
                metric,
                minimum_interval_overlap_bps,
                maximum_effect_delta_micros: _,
                minimum_p_value_micros,
            } => {
                validate_contract_text_api("tolerance.metric", metric)?;
                if *minimum_interval_overlap_bps > 10_000 || *minimum_p_value_micros > 1_000_000 {
                    return Err(ApiError::bad_request(
                        "invalid_statistical_tolerance",
                        "statistical overlap/p-value thresholds exceed fixed-point bounds",
                    ));
                }
                format!("statistical:{metric}")
            }
            ToleranceRule::Seed {
                expected_seed_set_hash,
            } => {
                validate_digest("expected_seed_set_hash", expected_seed_set_hash)?;
                format!("seed:{expected_seed_set_hash}")
            }
        };
        if !seen.insert(key) {
            return Err(ApiError::bad_request(
                "duplicate_tolerance_rule",
                "tolerance policy contains a duplicate rule",
            ));
        }
    }
    record_hash(policy, "tolerance policy")
}

fn verify_signature(
    frame: &[u8],
    signature: &str,
    key: &VerifyingKey,
    code: &'static str,
    message: &'static str,
) -> Result<(), ApiError> {
    key.verify(frame, &decode_signature(signature)?)
        .map_err(|_| ApiError::forbidden(code, message))
}

#[derive(Clone)]
struct ReviewPaperContext {
    paper: PaperProject,
    team: ResearchTeam,
    release_candidate: crate::paper_raid_contracts::PaperReleaseCandidateV2,
    release_candidate_hash: String,
}

fn review_context_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
) -> Result<ReviewPaperContext, ApiError> {
    let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let team = memory
        .teams
        .get(&paper.team_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
    let revision_id = paper.release_candidate_revision_id.ok_or_else(|| {
        ApiError::conflict(
            "release_candidate_required",
            "Paper review requires a frozen release candidate",
        )
    })?;
    let revision = memory
        .revisions
        .get(&revision_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("release candidate revision record is missing"))?;
    let release_candidate = revision.release_candidate.clone().ok_or_else(|| {
        ApiError::internal("release candidate revision has no canonical candidate")
    })?;
    let release_candidate_hash = revision
        .release_candidate_hash
        .clone()
        .ok_or_else(|| ApiError::internal("release candidate revision has no canonical hash"))?;
    Ok(ReviewPaperContext {
        paper,
        team,
        release_candidate,
        release_candidate_hash,
    })
}

async fn review_context_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<ReviewPaperContext, ApiError> {
    let row = sqlx::query(
        "select p.record_json as paper_record, t.record_json as team_record
         from hepta_paper_projects p
         join hepta_research_teams t on t.team_id=p.team_id
         where p.paper_project_id=$1 for share of p,t",
    )
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(row.get("paper_record"), "paper project")?;
    let team: ResearchTeam = decode_record(row.get("team_record"), "research team")?;
    let revision_id = paper.release_candidate_revision_id.ok_or_else(|| {
        ApiError::conflict(
            "release_candidate_required",
            "Paper review requires a frozen release candidate",
        )
    })?;
    let revision_row = sqlx::query(
        "select record_json from hepta_paper_revisions
         where revision_id=$1 and paper_project_id=$2 for share",
    )
    .bind(revision_id)
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::internal("release candidate revision record is missing"))?;
    let revision: PaperRevision = decode_record(revision_row.get("record_json"), "paper revision")?;
    let release_candidate = revision.release_candidate.clone().ok_or_else(|| {
        ApiError::internal("release candidate revision has no canonical candidate")
    })?;
    let release_candidate_hash = revision
        .release_candidate_hash
        .clone()
        .ok_or_else(|| ApiError::internal("release candidate revision has no canonical hash"))?;
    Ok(ReviewPaperContext {
        paper,
        team,
        release_candidate,
        release_candidate_hash,
    })
}

fn validate_request_key(
    signing_public_key: &str,
    signing_public_key_hash: &str,
) -> Result<VerifyingKey, ApiError> {
    validate_digest("signing_public_key_hash", signing_public_key_hash)?;
    let canonical = canonical_public_key(signing_public_key)?;
    let key = crate::decode_verifying_key(&canonical)?;
    if sha256_digest(&key.to_bytes()) != signing_public_key_hash {
        return Err(ApiError::bad_request(
            "signing_public_key_hash_mismatch",
            "signing_public_key_hash does not match the supplied Ed25519 key",
        ));
    }
    Ok(key)
}

fn active_player_key_memory(
    memory: &PaperRaidMemory,
    player_id: Uuid,
    signing_key_id: &str,
    signing_public_key: &str,
    signing_public_key_hash: &str,
) -> Result<(HumanPlayer, VerifyingKey), ApiError> {
    validate_contract_text_api("signing_key_id", signing_key_id)?;
    let key = validate_request_key(signing_public_key, signing_public_key_hash)?;
    let player = memory.players.get(&player_id).cloned().ok_or_else(|| {
        ApiError::not_found("human_player_not_found", "human player does not exist")
    })?;
    let registered = memory
        .human_signing_keys
        .get(&(player_id, signing_key_id.to_string()))
        .ok_or_else(|| {
            ApiError::forbidden(
                "human_signing_key_not_registered",
                "signing key is not registered for this player",
            )
        })?;
    if player.status != HumanPlayerStatus::Active
        || registered.status != HumanSigningKeyStatus::Active
        || registered.signing_public_key != signing_public_key
        || registered.signing_public_key_hash != signing_public_key_hash
        || player.signing_key_id != signing_key_id
        || player.signing_public_key_hash != signing_public_key_hash
    {
        return Err(ApiError::forbidden(
            "human_signing_key_not_current",
            "signature must use the player's current active key snapshot",
        ));
    }
    Ok((player, key))
}

async fn active_player_key_postgres(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    signing_key_id: &str,
    signing_public_key: &str,
    signing_public_key_hash: &str,
) -> Result<(HumanPlayer, VerifyingKey), ApiError> {
    validate_contract_text_api("signing_key_id", signing_key_id)?;
    let key = validate_request_key(signing_public_key, signing_public_key_hash)?;
    let row = sqlx::query(
        "select p.record_json as player_record, k.record_json as key_record
         from hepta_human_players p
         join hepta_human_signing_keys k
           on k.player_id=p.player_id and k.signing_key_id=$2
         where p.player_id=$1 for share of p,k",
    )
    .bind(player_id)
    .bind(signing_key_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "human_signing_key_not_registered",
            "signing key is not registered for this player",
        )
    })?;
    let player: HumanPlayer = decode_record(row.get("player_record"), "human player")?;
    let registered: HumanSigningKey = decode_record(row.get("key_record"), "human signing key")?;
    if player.status != HumanPlayerStatus::Active
        || registered.status != HumanSigningKeyStatus::Active
        || registered.signing_public_key != signing_public_key
        || registered.signing_public_key_hash != signing_public_key_hash
        || player.signing_key_id != signing_key_id
        || player.signing_public_key_hash != signing_public_key_hash
    {
        return Err(ApiError::forbidden(
            "human_signing_key_not_current",
            "signature must use the player's current active key snapshot",
        ));
    }
    Ok((player, key))
}

fn normalize_string_set(field: &'static str, values: &[String]) -> Result<Vec<String>, ApiError> {
    if values.is_empty() || values.len() > 32 {
        return Err(ApiError::bad_request(
            "invalid_contribution_roles",
            format!("{field} must contain 1-32 values"),
        ));
    }
    let mut result = values.to_vec();
    for value in &result {
        validate_contract_text_api(field, value)?;
    }
    result.sort();
    if result.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ApiError::bad_request(
            "duplicate_contribution_role",
            format!("{field} contains a duplicate value"),
        ));
    }
    Ok(result)
}

fn normalize_uuid_set(
    field: &'static str,
    values: &[Uuid],
    maximum: usize,
) -> Result<Vec<Uuid>, ApiError> {
    if values.len() > maximum {
        return Err(ApiError::bad_request(
            "contribution_reference_limit",
            format!("{field} exceeds the {maximum} item limit"),
        ));
    }
    let mut result = values.to_vec();
    result.sort();
    if result.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ApiError::bad_request(
            "duplicate_contribution_reference",
            format!("{field} contains a duplicate identifier"),
        ));
    }
    Ok(result)
}

fn contribution_ledger_hash(
    contribution_ledger_id: Uuid,
    paper_id: Uuid,
    entries: &[CreditContribution],
) -> Result<String, ApiError> {
    record_hash(
        &json!({
            "schema": CONTRIBUTION_LEDGER_SCHEMA_V1,
            "contribution_ledger_id": contribution_ledger_id,
            "paper_project_id": paper_id,
            "entries": entries,
        }),
        "contribution ledger",
    )
}

fn contribution_entry_skeletons(
    context: &ReviewPaperContext,
    request_entries: &[CreditContributionInput],
) -> Result<Vec<CreditContribution>, ApiError> {
    if request_entries.len() != context.team.members.len() {
        return Err(ApiError::bad_request(
            "incomplete_contribution_ledger",
            "contribution ledger must contain exactly one entry per team member",
        ));
    }
    let authors: HashMap<_, _> = context
        .release_candidate
        .authors
        .iter()
        .map(|author| (author.player_id, author))
        .collect();
    let team_players: HashSet<_> = context
        .team
        .members
        .iter()
        .map(|member| member.player_id)
        .collect();
    let mut seen = HashSet::new();
    let mut entries = Vec::with_capacity(request_entries.len());
    for input in request_entries {
        if !team_players.contains(&input.player_id) || !seen.insert(input.player_id) {
            return Err(ApiError::bad_request(
                "invalid_contribution_roster",
                "contribution entries must exactly match unique team members",
            ));
        }
        let author = authors.get(&input.player_id).ok_or_else(|| {
            ApiError::conflict(
                "release_author_roster_mismatch",
                "release candidate authors do not match the research team",
            )
        })?;
        let roles = normalize_string_set("credit_roles", &input.credit_roles)?;
        if roles != normalize_string_set("release_credit_roles", &author.credit_roles)? {
            return Err(ApiError::conflict(
                "credit_roles_mismatch",
                "contribution roles must exactly match the frozen release candidate",
            ));
        }
        let artifacts = normalize_uuid_set(
            "accepted_artifact_manifest_ids",
            &input.accepted_artifact_manifest_ids,
            256,
        )?;
        let reviews = normalize_uuid_set(
            "accepted_section_review_ids",
            &input.accepted_section_review_ids,
            256,
        )?;
        let points = u64::try_from(artifacts.len())
            .map_err(|_| ApiError::internal("artifact count overflow"))?
            .checked_mul(100)
            .and_then(|base| {
                u64::try_from(reviews.len())
                    .ok()
                    .and_then(|count| count.checked_mul(150))
                    .and_then(|extra| base.checked_add(extra))
            })
            .ok_or_else(|| ApiError::internal("contribution score overflow"))?;
        entries.push(CreditContribution {
            player_id: input.player_id,
            credit_roles: roles,
            accepted_artifact_manifest_ids: artifacts,
            accepted_section_review_ids: reviews,
            contribution_points: points,
        });
    }
    entries.sort_by_key(|entry| entry.player_id);
    Ok(entries)
}

fn validate_contribution_refs_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    entries: &[CreditContribution],
) -> Result<(), ApiError> {
    for entry in entries {
        for artifact_id in &entry.accepted_artifact_manifest_ids {
            let same_paper = memory
                .collaboration
                .artifact_manifests
                .get(artifact_id)
                .is_some_and(|manifest| manifest.paper_project_id == paper_id);
            let accepted = memory.collaboration.proposals.values().any(|proposal| {
                proposal.paper_project_id == paper_id
                    && proposal.artifact_manifest_id == *artifact_id
                    && proposal.status == AgentProposalStatus::Accepted
                    && memory.collaboration.decisions.values().any(|decision| {
                        decision.proposal_id == proposal.proposal_id
                            && decision.player_id == entry.player_id
                            && decision.decision == HumanDecisionKind::Accept
                    })
            });
            if !same_paper || !accepted {
                return Err(ApiError::conflict(
                    "artifact_not_accepted_by_contributor",
                    "artifact must be same-paper and accepted by the credited player",
                ));
            }
        }
        for review_id in &entry.accepted_section_review_ids {
            if memory
                .collaboration
                .section_reviews
                .get(review_id)
                .is_none_or(|review| {
                    review.paper_project_id != paper_id
                        || review.reviewer_player_id != entry.player_id
                        || review.verdict != SectionReviewVerdict::Approve
                })
            {
                return Err(ApiError::conflict(
                    "review_not_accepted_contribution",
                    "review must be an approving same-paper review by the credited player",
                ));
            }
        }
    }
    Ok(())
}

async fn validate_contribution_refs_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    entries: &[CreditContribution],
) -> Result<(), ApiError> {
    for entry in entries {
        for artifact_id in &entry.accepted_artifact_manifest_ids {
            let accepted = sqlx::query_scalar::<_, bool>(
                "select exists(
                    select 1 from hepta_artifact_manifests m
                    join hepta_agent_proposals p on p.artifact_manifest_id=m.manifest_id
                      and p.paper_project_id=m.paper_project_id and p.status='accepted'
                    join hepta_human_decisions d on d.proposal_id=p.proposal_id
                      and d.paper_project_id=p.paper_project_id and d.decision='accept'
                    where m.manifest_id=$1 and m.paper_project_id=$2 and d.player_id=$3
                 )",
            )
            .bind(artifact_id)
            .bind(paper_id)
            .bind(entry.player_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::database)?;
            if !accepted {
                return Err(ApiError::conflict(
                    "artifact_not_accepted_by_contributor",
                    "artifact must be same-paper and accepted by the credited player",
                ));
            }
        }
        for review_id in &entry.accepted_section_review_ids {
            let accepted = sqlx::query_scalar::<_, bool>(
                "select exists(select 1 from hepta_section_reviews
                   where review_id=$1 and paper_project_id=$2
                     and reviewer_player_id=$3 and verdict='approve')",
            )
            .bind(review_id)
            .bind(paper_id)
            .bind(entry.player_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::database)?;
            if !accepted {
                return Err(ApiError::conflict(
                    "review_not_accepted_contribution",
                    "review must be an approving same-paper review by the credited player",
                ));
            }
        }
    }
    Ok(())
}

async fn create_contribution_ledger(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateContributionLedgerRequest>,
) -> Result<(StatusCode, Json<ContributionLedger>), ApiError> {
    const OPERATION: &str = "create_contribution_ledger_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest("release_candidate_hash", &request.release_candidate_hash)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/contribution-ledgers");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    let now = Utc::now();

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        assert_team_actor_memory(&next, &context.team, &assertion)?;
        validate_contribution_context(&context, &request)?;
        if next
            .review
            .contribution_ledgers
            .values()
            .any(|ledger| ledger.paper_project_id == paper_id)
        {
            return Err(ApiError::conflict(
                "contribution_ledger_exists",
                "paper already has a frozen contribution ledger",
            ));
        }
        let entries = contribution_entry_skeletons(&context, &request.entries)?;
        validate_contribution_refs_memory(&next, paper_id, &entries)?;
        let ledger = make_contribution_ledger(&context, &request, entries, now)?;
        if next
            .review
            .contribution_ledgers
            .insert(ledger.contribution_ledger_id, ledger.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "contribution_ledger_exists",
                "contribution_ledger_id already exists",
            ));
        }
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.contribution_ledger.frozen.v1",
            paper_id,
            paper_id,
            1,
            json!({"ledger_hash": ledger.ledger_hash}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &ledger,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(ledger)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let context = review_context_postgres(&mut tx, paper_id).await?;
    assert_team_actor_postgres(&mut tx, context.team.team_id, &assertion).await?;
    validate_contribution_context(&context, &request)?;
    let entries = contribution_entry_skeletons(&context, &request.entries)?;
    validate_contribution_refs_postgres(&mut tx, paper_id, &entries).await?;
    let ledger = make_contribution_ledger(&context, &request, entries, now)?;
    let result = sqlx::query(
        "insert into hepta_paper_contribution_ledgers
         (contribution_ledger_id,paper_project_id,release_candidate_hash,ledger_hash,
          version,record_json,created_at)
         values ($1,$2,$3,$4,1,$5::jsonb,$6)",
    )
    .bind(ledger.contribution_ledger_id)
    .bind(paper_id)
    .bind(&ledger.release_candidate_hash)
    .bind(&ledger.ledger_hash)
    .bind(
        serde_json::to_value(&ledger)
            .map_err(|error| ApiError::internal(format!("encode contribution ledger: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "contribution_ledger_exists",
                "paper or contribution_ledger_id already has a frozen ledger",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.contribution_ledger.frozen.v1",
        paper_id,
        paper_id,
        1,
        json!({"ledger_hash": ledger.ledger_hash}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(ledger.contribution_ledger_id),
        StatusCode::CREATED,
        &ledger,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(ledger)))
}

fn validate_contribution_context(
    context: &ReviewPaperContext,
    request: &CreateContributionLedgerRequest,
) -> Result<(), ApiError> {
    if context.paper.version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            context.paper.version,
        ));
    }
    if context.paper.phase != PaperPhase::AuthorApproval {
        return Err(ApiError::conflict(
            "contribution_ledger_phase_closed",
            "contribution ledger may only be frozen during author approval",
        ));
    }
    if context.release_candidate_hash != request.release_candidate_hash {
        return Err(ApiError::conflict(
            "release_candidate_hash_mismatch",
            "request does not bind the current frozen release candidate",
        ));
    }
    Ok(())
}

fn make_contribution_ledger(
    context: &ReviewPaperContext,
    request: &CreateContributionLedgerRequest,
    entries: Vec<CreditContribution>,
    now: DateTime<Utc>,
) -> Result<ContributionLedger, ApiError> {
    let ledger_hash = contribution_ledger_hash(
        request.contribution_ledger_id,
        context.paper.paper_project_id,
        &entries,
    )?;
    if ledger_hash != context.release_candidate.contribution_ledger_hash {
        return Err(ApiError::conflict(
            "contribution_ledger_hash_mismatch",
            "derived contribution ledger does not match the frozen release candidate",
        ));
    }
    Ok(ContributionLedger {
        schema: CONTRIBUTION_LEDGER_SCHEMA_V1.to_string(),
        contribution_ledger_id: request.contribution_ledger_id,
        paper_project_id: context.paper.paper_project_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        entries,
        ledger_hash,
        version: 1,
        created_at: now,
    })
}

struct PreparedEvaluation {
    tolerance_policy_hash: String,
    paper_score: PaperScore,
    status: PaperEvaluationStatus,
    evaluator_frame: Vec<u8>,
    evaluation_signing_hash: String,
}

fn load_submission_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    submission_id: Uuid,
) -> Result<JointPaperSubmission, ApiError> {
    memory
        .submissions
        .get(&submission_id)
        .filter(|submission| submission.paper_project_id == paper_id)
        .cloned()
        .ok_or_else(|| {
            ApiError::not_found(
                "joint_submission_not_found",
                "joint paper submission does not exist for this paper",
            )
        })
}

async fn load_submission_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    submission_id: Uuid,
) -> Result<JointPaperSubmission, ApiError> {
    let row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where submission_id=$1 and paper_project_id=$2 for share",
    )
    .bind(submission_id)
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "joint_submission_not_found",
            "joint paper submission does not exist for this paper",
        )
    })?;
    decode_record(row.get("record_json"), "joint paper submission")
}

fn load_contribution_ledger_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<ContributionLedger, ApiError> {
    memory
        .review
        .contribution_ledgers
        .values()
        .find(|ledger| {
            ledger.paper_project_id == paper_id
                && ledger.release_candidate_hash == release_candidate_hash
        })
        .cloned()
        .ok_or_else(|| {
            ApiError::conflict(
                "contribution_ledger_required",
                "evaluation requires the frozen contribution ledger",
            )
        })
}

async fn load_contribution_ledger_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<ContributionLedger, ApiError> {
    let row = sqlx::query(
        "select record_json from hepta_paper_contribution_ledgers
         where paper_project_id=$1 and release_candidate_hash=$2 for share",
    )
    .bind(paper_id)
    .bind(release_candidate_hash)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "contribution_ledger_required",
            "evaluation requires the frozen contribution ledger",
        )
    })?;
    decode_record(row.get("record_json"), "contribution ledger")
}

fn validate_evaluation_context(
    context: &ReviewPaperContext,
    submission: &JointPaperSubmission,
    ledger: &ContributionLedger,
    request: &CreatePaperEvaluationRequest,
    now: DateTime<Utc>,
) -> Result<PreparedEvaluation, ApiError> {
    if context.paper.phase != PaperPhase::SubmissionReady
        || submission.status != JointSubmissionStatus::SubmissionReady
    {
        return Err(ApiError::conflict(
            "paper_not_submission_ready",
            "evaluation requires a finalized submission-ready PaperBundle",
        ));
    }
    if context.release_candidate_hash != request.release_candidate_hash
        || submission.release_candidate_hash != request.release_candidate_hash
        || submission.paper_bundle_hash != request.paper_bundle_hash
        || submission.paper_bundle.release_candidate_hash != request.release_candidate_hash
        || submission.paper_bundle.paper_bundle_hash != request.paper_bundle_hash
    {
        return Err(ApiError::conflict(
            "evaluation_release_mismatch",
            "evaluation must bind the exact frozen release candidate and PaperBundle",
        ));
    }
    if ledger.ledger_hash != context.release_candidate.contribution_ledger_hash {
        return Err(ApiError::conflict(
            "contribution_ledger_hash_mismatch",
            "frozen contribution ledger does not match the PaperBundle",
        ));
    }
    if request.reference_metrics_micros.is_empty() || request.reference_metrics_micros.len() > 256 {
        return Err(ApiError::bad_request(
            "invalid_reference_metrics",
            "reference metrics must contain 1-256 fixed-point values",
        ));
    }
    for metric in request.reference_metrics_micros.keys() {
        validate_contract_text_api("reference_metric", metric)?;
    }
    let tolerance_policy_hash = validate_tolerance_policy(&request.tolerance_policy)?;
    if request.tolerance_policy.version != "1" {
        return Err(ApiError::bad_request(
            "unsupported_tolerance_policy_version",
            "only frozen tolerance policy version 1 is supported",
        ));
    }
    for rule in &request.tolerance_policy.rules {
        let metric = match rule {
            ToleranceRule::Absolute { metric, .. }
            | ToleranceRule::Relative { metric, .. }
            | ToleranceRule::Statistical { metric, .. } => Some(metric),
            ToleranceRule::Seed { .. } => None,
        };
        if metric.is_some_and(|metric| !request.reference_metrics_micros.contains_key(metric)) {
            return Err(ApiError::bad_request(
                "tolerance_metric_not_frozen",
                "every tolerance metric must have a frozen reference value",
            ));
        }
    }
    if request.reviewer_attestations.len() != 2 {
        return Err(ApiError::bad_request(
            "review_panel_size_invalid",
            "evaluation requires exactly two independent reviewer attestations",
        ));
    }
    let authors: HashSet<_> = context
        .team
        .members
        .iter()
        .map(|member| member.player_id)
        .collect();
    let reviewers: HashSet<_> = request
        .reviewer_attestations
        .iter()
        .map(|review| review.reviewer_player_id)
        .collect();
    if reviewers.len() != 2
        || reviewers.contains(&request.evaluator_player_id)
        || authors.contains(&request.evaluator_player_id)
        || reviewers.iter().any(|reviewer| authors.contains(reviewer))
    {
        return Err(ApiError::forbidden(
            "review_panel_not_independent",
            "evaluator and exactly two reviewers must be distinct non-authors",
        ));
    }
    validate_digest(
        "evaluator_coi_attestation_hash",
        &request.evaluator_coi_attestation_hash,
    )?;
    signed_time(request.evaluator_signed_at_unix)?;
    for review in &request.reviewer_attestations {
        validate_digest(
            "reviewer_coi_attestation_hash",
            &review.coi_attestation_hash,
        )?;
        signed_time(review.signed_at_unix)?;
    }
    let score_bps = request.score_components.validate_and_total()?;
    let eligible = request.hard_gates.eligible();
    let score_hash = record_hash(
        &json!({
            "schema": PAPER_SCORE_SCHEMA_V1,
            "evaluation_id": request.evaluation_id,
            "paper_project_id": context.paper.paper_project_id,
            "components": request.score_components,
            "hard_gates": request.hard_gates,
            "score_bps": score_bps,
            "eligible": eligible,
        }),
        "paper score",
    )?;
    let paper_score = PaperScore {
        schema: PAPER_SCORE_SCHEMA_V1.to_string(),
        evaluation_id: request.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        components: request.score_components.clone(),
        hard_gates: request.hard_gates.clone(),
        score_bps,
        eligible,
        score_hash: score_hash.clone(),
        created_at: now,
    };
    let reference_metrics_hash =
        record_hash(&request.reference_metrics_micros, "reference metrics")?;
    let hard_gates_hash = record_hash(&request.hard_gates, "hard gates")?;
    let signing = PaperEvaluationSigningV1 {
        schema: PAPER_EVALUATION_V1.to_string(),
        evaluation_id: request.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        submission_id: request.submission_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: request.paper_bundle_hash.clone(),
        supersedes_evaluation_id: request.supersedes_evaluation_id,
        tolerance_policy_hash: tolerance_policy_hash.clone(),
        paper_score_hash: score_hash,
        reference_metrics_hash,
        hard_gates_hash,
        evaluator_player_id: request.evaluator_player_id,
        signing_key_id: request.evaluator_signing_key_id.clone(),
        signing_public_key_hash: request.evaluator_signing_public_key_hash.clone(),
        coi_attestation_hash: request.evaluator_coi_attestation_hash.clone(),
        signed_at_unix: request.evaluator_signed_at_unix,
    };
    let evaluator_frame = paper_evaluation_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_evaluation_contract", message))?;
    let evaluation_signing_hash = sha256_digest(&evaluator_frame);
    let approvals = request
        .reviewer_attestations
        .iter()
        .filter(|review| review.verdict == PanelVerdict::Approve)
        .count();
    let status = if !eligible {
        PaperEvaluationStatus::NotEligible
    } else if score_bps >= 6_000 && approvals == 2 {
        PaperEvaluationStatus::Accepted
    } else {
        PaperEvaluationStatus::Rejected
    };
    Ok(PreparedEvaluation {
        tolerance_policy_hash,
        paper_score,
        status,
        evaluator_frame,
        evaluation_signing_hash,
    })
}

fn review_attestation(
    request: &ReviewAttestationRequest,
    evaluation_id: Uuid,
    evaluation_signing_hash: &str,
    key: &VerifyingKey,
) -> Result<ReviewAttestation, ApiError> {
    let signing = PaperReviewAttestationSigningV1 {
        schema: PAPER_REVIEW_ATTESTATION_V1.to_string(),
        attestation_id: request.attestation_id,
        evaluation_id,
        evaluation_signing_hash: evaluation_signing_hash.to_string(),
        reviewer_player_id: request.reviewer_player_id,
        verdict: request.verdict.as_str().to_string(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        coi_attestation_hash: request.coi_attestation_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let frame = paper_review_attestation_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_review_contract", message))?;
    verify_signature(
        &frame,
        &request.signature,
        key,
        "review_signature_failed",
        "reviewer attestation signature verification failed",
    )?;
    Ok(ReviewAttestation {
        attestation_id: request.attestation_id,
        evaluation_id,
        evaluation_signing_hash: evaluation_signing_hash.to_string(),
        reviewer_player_id: request.reviewer_player_id,
        verdict: request.verdict.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        coi_attestation_hash: request.coi_attestation_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
    })
}

fn make_evaluation(
    context: &ReviewPaperContext,
    request: &CreatePaperEvaluationRequest,
    prepared: PreparedEvaluation,
    mut attestations: Vec<ReviewAttestation>,
    now: DateTime<Utc>,
    version: u64,
) -> PaperEvaluation {
    attestations.sort_by_key(|review| review.reviewer_player_id);
    PaperEvaluation {
        schema: PAPER_EVALUATION_V1.to_string(),
        evaluation_id: request.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        submission_id: request.submission_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: request.paper_bundle_hash.clone(),
        supersedes_evaluation_id: request.supersedes_evaluation_id,
        tolerance_policy: request.tolerance_policy.clone(),
        tolerance_policy_hash: prepared.tolerance_policy_hash,
        reference_metrics_micros: request.reference_metrics_micros.clone(),
        evaluator_player_id: request.evaluator_player_id,
        evaluator_signing_key_id: request.evaluator_signing_key_id.clone(),
        evaluator_signing_public_key: request.evaluator_signing_public_key.clone(),
        evaluator_signing_public_key_hash: request.evaluator_signing_public_key_hash.clone(),
        evaluator_coi_attestation_hash: request.evaluator_coi_attestation_hash.clone(),
        evaluator_signed_at_unix: request.evaluator_signed_at_unix,
        evaluator_signature: request.evaluator_signature.clone(),
        reviewer_attestations: attestations,
        paper_score: prepared.paper_score,
        status: prepared.status,
        settlement_state: ReviewSettlementState::PendingFinality,
        evaluation_signing_hash: prepared.evaluation_signing_hash,
        version,
        created_at: now,
    }
}

fn make_raid_score(
    evaluation: &PaperEvaluation,
    ledger: &ContributionLedger,
    now: DateTime<Utc>,
) -> Result<RaidScore, ApiError> {
    let player_xp: BTreeMap<_, _> = ledger
        .entries
        .iter()
        .map(|entry| (entry.player_id, entry.contribution_points))
        .collect();
    let team_xp = player_xp
        .values()
        .try_fold(1_000_u64, |total, points| {
            total.checked_add(*points).ok_or(())
        })
        .map_err(|_| ApiError::internal("raid score overflow"))?;
    let score_hash = record_hash(
        &json!({
            "schema": RAID_SCORE_SCHEMA_V1,
            "raid_score_id": evaluation.evaluation_id,
            "evaluation_id": evaluation.evaluation_id,
            "paper_project_id": evaluation.paper_project_id,
            "team_xp": team_xp,
            "player_xp": player_xp,
            "paper_score_excluded": true,
        }),
        "raid score",
    )?;
    Ok(RaidScore {
        schema: RAID_SCORE_SCHEMA_V1.to_string(),
        raid_score_id: evaluation.evaluation_id,
        evaluation_id: evaluation.evaluation_id,
        paper_project_id: evaluation.paper_project_id,
        team_xp,
        player_xp,
        paper_score_excluded: true,
        score_hash,
        created_at: now,
    })
}

fn panel_members(evaluation: &PaperEvaluation) -> HashSet<Uuid> {
    std::iter::once(evaluation.evaluator_player_id)
        .chain(
            evaluation
                .reviewer_attestations
                .iter()
                .map(|review| review.reviewer_player_id),
        )
        .collect()
}

fn validate_supersession_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    request: &CreatePaperEvaluationRequest,
) -> Result<u64, ApiError> {
    let Some(base_id) = request.supersedes_evaluation_id else {
        if memory.review.evaluations.values().any(|evaluation| {
            evaluation.paper_project_id == paper_id
                && evaluation.submission_id == request.submission_id
                && evaluation.supersedes_evaluation_id.is_none()
        }) {
            return Err(ApiError::conflict(
                "initial_evaluation_exists",
                "submission already has an initial immutable evaluation",
            ));
        }
        return Ok(1);
    };
    let base = memory.review.evaluations.get(&base_id).ok_or_else(|| {
        ApiError::not_found(
            "superseded_evaluation_not_found",
            "base evaluation does not exist",
        )
    })?;
    if base.paper_project_id != paper_id
        || base.submission_id != request.submission_id
        || base.release_candidate_hash != request.release_candidate_hash
        || base.paper_bundle_hash != request.paper_bundle_hash
    {
        return Err(ApiError::conflict(
            "superseding_evaluation_scope_mismatch",
            "superseding evaluation must bind the same paper, submission, and release",
        ));
    }
    let open_appeal = memory.review.appeals.values().any(|appeal| {
        appeal.evaluation_id == base_id
            && !memory
                .review
                .resolutions
                .values()
                .any(|resolution| resolution.appeal_id == appeal.appeal_id)
    });
    if !open_appeal {
        return Err(ApiError::conflict(
            "open_appeal_required",
            "an immutable evaluation may only be superseded during an open Appeal",
        ));
    }
    if memory
        .review
        .evaluations
        .values()
        .any(|evaluation| evaluation.supersedes_evaluation_id == Some(base_id))
    {
        return Err(ApiError::conflict(
            "evaluation_already_superseded",
            "base evaluation already has a superseding evaluation",
        ));
    }
    let new_panel: HashSet<_> = std::iter::once(request.evaluator_player_id)
        .chain(
            request
                .reviewer_attestations
                .iter()
                .map(|review| review.reviewer_player_id),
        )
        .collect();
    if !panel_members(base).is_disjoint(&new_panel) {
        return Err(ApiError::forbidden(
            "superseding_panel_not_independent",
            "superseding evaluation must use a panel disjoint from the appealed panel",
        ));
    }
    base.version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("evaluation version overflow"))
}

async fn validate_supersession_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    request: &CreatePaperEvaluationRequest,
) -> Result<u64, ApiError> {
    let Some(base_id) = request.supersedes_evaluation_id else {
        let exists = sqlx::query_scalar::<_, bool>(
            "select exists(select 1 from hepta_paper_evaluations
              where submission_id=$1 and supersedes_evaluation_id is null)",
        )
        .bind(request.submission_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if exists {
            return Err(ApiError::conflict(
                "initial_evaluation_exists",
                "submission already has an initial immutable evaluation",
            ));
        }
        return Ok(1);
    };
    let row = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where evaluation_id=$1 for share",
    )
    .bind(base_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "superseded_evaluation_not_found",
            "base evaluation does not exist",
        )
    })?;
    let base: PaperEvaluation = decode_record(row.get("record_json"), "paper evaluation")?;
    if base.paper_project_id != paper_id
        || base.submission_id != request.submission_id
        || base.release_candidate_hash != request.release_candidate_hash
        || base.paper_bundle_hash != request.paper_bundle_hash
    {
        return Err(ApiError::conflict(
            "superseding_evaluation_scope_mismatch",
            "superseding evaluation must bind the same paper, submission, and release",
        ));
    }
    let open_appeal = sqlx::query_scalar::<_, bool>(
        "select exists(
           select 1 from hepta_paper_appeals a
           left join hepta_paper_appeal_resolutions r on r.appeal_id=a.appeal_id
           where a.evaluation_id=$1 and r.appeal_id is null)",
    )
    .bind(base_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if !open_appeal {
        return Err(ApiError::conflict(
            "open_appeal_required",
            "an immutable evaluation may only be superseded during an open Appeal",
        ));
    }
    let new_panel: HashSet<_> = std::iter::once(request.evaluator_player_id)
        .chain(
            request
                .reviewer_attestations
                .iter()
                .map(|review| review.reviewer_player_id),
        )
        .collect();
    if !panel_members(&base).is_disjoint(&new_panel) {
        return Err(ApiError::forbidden(
            "superseding_panel_not_independent",
            "superseding evaluation must use a panel disjoint from the appealed panel",
        ));
    }
    base.version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("evaluation version overflow"))
}

async fn create_paper_evaluation(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperEvaluationRequest>,
) -> Result<(StatusCode, Json<PaperEvaluation>), ApiError> {
    const OPERATION: &str = "create_paper_evaluation_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    for (field, value) in [
        (
            "release_candidate_hash",
            request.release_candidate_hash.as_str(),
        ),
        ("paper_bundle_hash", request.paper_bundle_hash.as_str()),
        (
            "evaluator_coi_attestation_hash",
            request.evaluator_coi_attestation_hash.as_str(),
        ),
    ] {
        validate_digest(field, value)?;
    }
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evaluations");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.evaluator_player_id {
        return Err(ApiError::forbidden(
            "evaluator_assertion_mismatch",
            "Consumer assertion must identify the evaluator",
        ));
    }
    let now = Utc::now();

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let submission = load_submission_memory(&next, paper_id, request.submission_id)?;
        let ledger =
            load_contribution_ledger_memory(&next, paper_id, &request.release_candidate_hash)?;
        let version = validate_supersession_memory(&next, paper_id, &request)?;
        let prepared = validate_evaluation_context(&context, &submission, &ledger, &request, now)?;
        let (evaluator, evaluator_key) = active_player_key_memory(
            &next,
            request.evaluator_player_id,
            &request.evaluator_signing_key_id,
            &request.evaluator_signing_public_key,
            &request.evaluator_signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &evaluator)?;
        verify_signature(
            &prepared.evaluator_frame,
            &request.evaluator_signature,
            &evaluator_key,
            "evaluation_signature_failed",
            "evaluator signature verification failed",
        )?;
        let mut seen_attestations = HashSet::new();
        let mut attestations = Vec::with_capacity(2);
        for review in &request.reviewer_attestations {
            if !seen_attestations.insert(review.attestation_id) {
                return Err(ApiError::bad_request(
                    "duplicate_review_attestation",
                    "review attestation IDs must be unique",
                ));
            }
            let (_, key) = active_player_key_memory(
                &next,
                review.reviewer_player_id,
                &review.signing_key_id,
                &review.signing_public_key,
                &review.signing_public_key_hash,
            )?;
            attestations.push(review_attestation(
                review,
                request.evaluation_id,
                &prepared.evaluation_signing_hash,
                &key,
            )?);
        }
        let evaluation = make_evaluation(&context, &request, prepared, attestations, now, version);
        let raid_score = make_raid_score(&evaluation, &ledger, now)?;
        if next
            .review
            .evaluations
            .insert(evaluation.evaluation_id, evaluation.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "paper_evaluation_exists",
                "evaluation_id already exists",
            ));
        }
        next.review
            .raid_scores
            .insert(raid_score.raid_score_id, raid_score);
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.evaluation.recorded.v1",
            paper_id,
            paper_id,
            evaluation.version,
            json!({
                "evaluation_id": evaluation.evaluation_id,
                "status": evaluation.status,
                "settlement_state": evaluation.settlement_state,
                "paper_score_hash": evaluation.paper_score.score_hash,
            }),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &evaluation,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(evaluation)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission = load_submission_postgres(&mut tx, paper_id, request.submission_id).await?;
    let ledger =
        load_contribution_ledger_postgres(&mut tx, paper_id, &request.release_candidate_hash)
            .await?;
    let version = validate_supersession_postgres(&mut tx, paper_id, &request).await?;
    let prepared = validate_evaluation_context(&context, &submission, &ledger, &request, now)?;
    let (evaluator, evaluator_key) = active_player_key_postgres(
        &mut tx,
        request.evaluator_player_id,
        &request.evaluator_signing_key_id,
        &request.evaluator_signing_public_key,
        &request.evaluator_signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &evaluator)?;
    verify_signature(
        &prepared.evaluator_frame,
        &request.evaluator_signature,
        &evaluator_key,
        "evaluation_signature_failed",
        "evaluator signature verification failed",
    )?;
    let mut seen_attestations = HashSet::new();
    let mut attestations = Vec::with_capacity(2);
    for review in &request.reviewer_attestations {
        if !seen_attestations.insert(review.attestation_id) {
            return Err(ApiError::bad_request(
                "duplicate_review_attestation",
                "review attestation IDs must be unique",
            ));
        }
        let (_, key) = active_player_key_postgres(
            &mut tx,
            review.reviewer_player_id,
            &review.signing_key_id,
            &review.signing_public_key,
            &review.signing_public_key_hash,
        )
        .await?;
        attestations.push(review_attestation(
            review,
            request.evaluation_id,
            &prepared.evaluation_signing_hash,
            &key,
        )?);
    }
    let evaluation = make_evaluation(&context, &request, prepared, attestations, now, version);
    let raid_score = make_raid_score(&evaluation, &ledger, now)?;
    insert_evaluation_postgres(&mut tx, &evaluation, &raid_score).await?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.evaluation.recorded.v1",
        paper_id,
        paper_id,
        evaluation.version,
        json!({
            "evaluation_id": evaluation.evaluation_id,
            "status": evaluation.status,
            "settlement_state": evaluation.settlement_state,
            "paper_score_hash": evaluation.paper_score.score_hash,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(evaluation.evaluation_id),
        StatusCode::CREATED,
        &evaluation,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(evaluation)))
}

async fn insert_evaluation_postgres(
    tx: &mut Transaction<'_, Postgres>,
    evaluation: &PaperEvaluation,
    raid_score: &RaidScore,
) -> Result<(), ApiError> {
    let result = sqlx::query(
        "insert into hepta_paper_evaluations (
            evaluation_id,paper_project_id,submission_id,release_candidate_hash,
            paper_bundle_hash,supersedes_evaluation_id,evaluator_player_id,
            tolerance_policy_hash,paper_score_hash,score_bps,eligible,status,
            settlement_state,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15::jsonb,$16)",
    )
    .bind(evaluation.evaluation_id)
    .bind(evaluation.paper_project_id)
    .bind(evaluation.submission_id)
    .bind(&evaluation.release_candidate_hash)
    .bind(&evaluation.paper_bundle_hash)
    .bind(evaluation.supersedes_evaluation_id)
    .bind(evaluation.evaluator_player_id)
    .bind(&evaluation.tolerance_policy_hash)
    .bind(&evaluation.paper_score.score_hash)
    .bind(i32::from(evaluation.paper_score.score_bps))
    .bind(evaluation.paper_score.eligible)
    .bind(evaluation.status.as_str())
    .bind(evaluation.settlement_state.as_str())
    .bind(i64::try_from(evaluation.version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(
        serde_json::to_value(evaluation)
            .map_err(|error| ApiError::internal(format!("encode paper evaluation: {error}")))?,
    )
    .bind(evaluation.created_at)
    .execute(&mut **tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_evaluation_exists",
                "evaluation identity or immutable supersession slot already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    for attestation in &evaluation.reviewer_attestations {
        sqlx::query(
            "insert into hepta_paper_evaluation_panel_attestations (
                attestation_id,evaluation_id,paper_project_id,reviewer_player_id,
                verdict,coi_attestation_hash,signing_key_id,signing_public_key_hash,
                signature,record_json,signed_at
             ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11)",
        )
        .bind(attestation.attestation_id)
        .bind(evaluation.evaluation_id)
        .bind(evaluation.paper_project_id)
        .bind(attestation.reviewer_player_id)
        .bind(attestation.verdict.as_str())
        .bind(&attestation.coi_attestation_hash)
        .bind(&attestation.signing_key_id)
        .bind(&attestation.signing_public_key_hash)
        .bind(&attestation.signature)
        .bind(
            serde_json::to_value(attestation).map_err(|error| {
                ApiError::internal(format!("encode review attestation: {error}"))
            })?,
        )
        .bind(signed_time(attestation.signed_at_unix)?)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    }
    sqlx::query(
        "insert into hepta_paper_scores
         (evaluation_id,paper_project_id,score_bps,eligible,score_hash,record_json,created_at)
         values ($1,$2,$3,$4,$5,$6::jsonb,$7)",
    )
    .bind(evaluation.evaluation_id)
    .bind(evaluation.paper_project_id)
    .bind(i32::from(evaluation.paper_score.score_bps))
    .bind(evaluation.paper_score.eligible)
    .bind(&evaluation.paper_score.score_hash)
    .bind(
        serde_json::to_value(&evaluation.paper_score)
            .map_err(|error| ApiError::internal(format!("encode paper score: {error}")))?,
    )
    .bind(evaluation.created_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    sqlx::query(
        "insert into hepta_paper_raid_scores
         (raid_score_id,evaluation_id,paper_project_id,team_xp,player_xp_json,
          score_hash,record_json,created_at)
         values ($1,$2,$3,$4,$5::jsonb,$6,$7::jsonb,$8)",
    )
    .bind(raid_score.raid_score_id)
    .bind(evaluation.evaluation_id)
    .bind(evaluation.paper_project_id)
    .bind(i64::try_from(raid_score.team_xp).map_err(|_| ApiError::internal("raid XP overflow"))?)
    .bind(
        serde_json::to_value(&raid_score.player_xp)
            .map_err(|error| ApiError::internal(format!("encode player XP: {error}")))?,
    )
    .bind(&raid_score.score_hash)
    .bind(
        serde_json::to_value(raid_score)
            .map_err(|error| ApiError::internal(format!("encode raid score: {error}")))?,
    )
    .bind(raid_score.created_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn evaluate_tolerance(
    evaluation: &PaperEvaluation,
    request: &CreatePaperReproductionRequest,
) -> Result<Vec<RuleResult>, ApiError> {
    let mut results = Vec::with_capacity(evaluation.tolerance_policy.rules.len());
    for rule in &evaluation.tolerance_policy.rules {
        let (rule_key, passed, details) = match rule {
            ToleranceRule::Absolute {
                metric,
                max_delta_micros,
            } => {
                let reference =
                    evaluation
                        .reference_metrics_micros
                        .get(metric)
                        .ok_or_else(|| {
                            ApiError::internal("absolute tolerance metric has no frozen reference")
                        })?;
                let observed = request.observed_metrics_micros.get(metric).ok_or_else(|| {
                    ApiError::bad_request(
                        "reproduction_metric_missing",
                        format!("observed metric {metric} is required"),
                    )
                })?;
                let delta = (i128::from(*observed) - i128::from(*reference)).abs();
                (
                    format!("absolute:{metric}"),
                    delta <= i128::from(*max_delta_micros),
                    json!({"reference":reference,"observed":observed,"delta":delta.to_string(),"maximum":max_delta_micros}),
                )
            }
            ToleranceRule::Relative {
                metric,
                max_delta_bps,
            } => {
                let reference =
                    evaluation
                        .reference_metrics_micros
                        .get(metric)
                        .ok_or_else(|| {
                            ApiError::internal("relative tolerance metric has no frozen reference")
                        })?;
                let observed = request.observed_metrics_micros.get(metric).ok_or_else(|| {
                    ApiError::bad_request(
                        "reproduction_metric_missing",
                        format!("observed metric {metric} is required"),
                    )
                })?;
                let delta = (i128::from(*observed) - i128::from(*reference)).abs();
                let denominator = i128::from(*reference).abs().max(1);
                let passed = delta
                    .checked_mul(10_000)
                    .is_some_and(|scaled| scaled <= denominator * i128::from(*max_delta_bps));
                (
                    format!("relative:{metric}"),
                    passed,
                    json!({"reference":reference,"observed":observed,"delta":delta.to_string(),"max_delta_bps":max_delta_bps}),
                )
            }
            ToleranceRule::Statistical {
                metric,
                minimum_interval_overlap_bps,
                maximum_effect_delta_micros,
                minimum_p_value_micros,
            } => {
                let evidence = request.statistical_evidence.get(metric).ok_or_else(|| {
                    ApiError::bad_request(
                        "statistical_evidence_missing",
                        format!("statistical evidence for {metric} is required"),
                    )
                })?;
                if evidence.interval_overlap_bps > 10_000 || evidence.p_value_micros > 1_000_000 {
                    return Err(ApiError::bad_request(
                        "statistical_evidence_out_of_range",
                        "statistical evidence exceeds fixed-point bounds",
                    ));
                }
                let effect_delta = i128::from(evidence.effect_delta_micros).abs();
                (
                    format!("statistical:{metric}"),
                    evidence.interval_overlap_bps >= *minimum_interval_overlap_bps
                        && effect_delta <= i128::from(*maximum_effect_delta_micros)
                        && evidence.p_value_micros >= *minimum_p_value_micros,
                    json!({
                        "interval_overlap_bps":evidence.interval_overlap_bps,
                        "minimum_interval_overlap_bps":minimum_interval_overlap_bps,
                        "effect_delta_micros":effect_delta.to_string(),
                        "maximum_effect_delta_micros":maximum_effect_delta_micros,
                        "p_value_micros":evidence.p_value_micros,
                        "minimum_p_value_micros":minimum_p_value_micros,
                    }),
                )
            }
            ToleranceRule::Seed {
                expected_seed_set_hash,
            } => (
                format!("seed:{expected_seed_set_hash}"),
                &request.seed_set_hash == expected_seed_set_hash,
                json!({"expected":expected_seed_set_hash,"observed":request.seed_set_hash}),
            ),
        };
        results.push(RuleResult {
            rule_key,
            passed,
            detail_hash: record_hash(&details, "tolerance rule result")?,
        });
    }
    Ok(results)
}

fn prepare_reproduction(
    context: &ReviewPaperContext,
    evaluation: &PaperEvaluation,
    request: &CreatePaperReproductionRequest,
    version: u64,
    now: DateTime<Utc>,
) -> Result<(PaperReproduction, Vec<u8>), ApiError> {
    for (field, value) in [
        (
            "release_candidate_hash",
            request.release_candidate_hash.as_str(),
        ),
        ("paper_bundle_hash", request.paper_bundle_hash.as_str()),
        ("seed_set_hash", request.seed_set_hash.as_str()),
        ("environment_hash", request.environment_hash.as_str()),
        ("run_manifest_hash", request.run_manifest_hash.as_str()),
        (
            "coi_attestation_hash",
            request.coi_attestation_hash.as_str(),
        ),
    ] {
        validate_digest(field, value)?;
    }
    signed_time(request.signed_at_unix)?;
    if evaluation.paper_project_id != context.paper.paper_project_id
        || evaluation.release_candidate_hash != request.release_candidate_hash
        || evaluation.paper_bundle_hash != request.paper_bundle_hash
        || context.release_candidate_hash != request.release_candidate_hash
    {
        return Err(ApiError::conflict(
            "reproduction_release_mismatch",
            "reproduction must bind the exact evaluated release and PaperBundle",
        ));
    }
    let authors: HashSet<_> = context
        .team
        .members
        .iter()
        .map(|member| member.player_id)
        .collect();
    if authors.contains(&request.reproducer_player_id)
        || panel_members(evaluation).contains(&request.reproducer_player_id)
    {
        return Err(ApiError::forbidden(
            "reproducer_not_independent",
            "reproducer must be distinct from authors, evaluator, and reviewers",
        ));
    }
    if request.observed_metrics_micros.len() > 256 || request.statistical_evidence.len() > 256 {
        return Err(ApiError::bad_request(
            "reproduction_metric_limit",
            "reproduction fixed-point metric maps exceed the 256 item limit",
        ));
    }
    for metric in request
        .observed_metrics_micros
        .keys()
        .chain(request.statistical_evidence.keys())
    {
        validate_contract_text_api("reproduction_metric", metric)?;
    }
    let rule_results = evaluate_tolerance(evaluation, request)?;
    let status = if rule_results.iter().all(|result| result.passed) {
        ReproductionStatus::Reproduced
    } else {
        ReproductionStatus::FailedTolerance
    };
    let observed_metrics_hash = record_hash(&request.observed_metrics_micros, "observed metrics")?;
    let statistical_evidence_hash =
        record_hash(&request.statistical_evidence, "statistical evidence")?;
    let signing = PaperReproductionSigningV1 {
        schema: PAPER_REPRODUCTION_V1.to_string(),
        reproduction_id: request.reproduction_id,
        evaluation_id: evaluation.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: request.paper_bundle_hash.clone(),
        tolerance_policy_hash: evaluation.tolerance_policy_hash.clone(),
        observed_metrics_hash,
        statistical_evidence_hash,
        seed_set_hash: request.seed_set_hash.clone(),
        environment_hash: request.environment_hash.clone(),
        run_manifest_hash: request.run_manifest_hash.clone(),
        supersedes_reproduction_id: request.supersedes_reproduction_id,
        reproducer_player_id: request.reproducer_player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        coi_attestation_hash: request.coi_attestation_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let frame = paper_reproduction_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_reproduction_contract", message))?;
    let report_hash = record_hash(
        &json!({
            "signing_hash": sha256_digest(&frame),
            "rule_results": rule_results,
            "status": status,
        }),
        "reproduction report",
    )?;
    Ok((
        PaperReproduction {
            schema: PAPER_REPRODUCTION_V1.to_string(),
            reproduction_id: request.reproduction_id,
            evaluation_id: evaluation.evaluation_id,
            paper_project_id: context.paper.paper_project_id,
            release_candidate_hash: request.release_candidate_hash.clone(),
            paper_bundle_hash: request.paper_bundle_hash.clone(),
            tolerance_policy_hash: evaluation.tolerance_policy_hash.clone(),
            observed_metrics_micros: request.observed_metrics_micros.clone(),
            statistical_evidence: request.statistical_evidence.clone(),
            seed_set_hash: request.seed_set_hash.clone(),
            environment_hash: request.environment_hash.clone(),
            run_manifest_hash: request.run_manifest_hash.clone(),
            supersedes_reproduction_id: request.supersedes_reproduction_id,
            reproducer_player_id: request.reproducer_player_id,
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            coi_attestation_hash: request.coi_attestation_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            rule_results,
            status,
            report_hash,
            version,
            created_at: now,
        },
        frame,
    ))
}

fn reproduction_version_memory(
    memory: &PaperRaidMemory,
    evaluation_id: Uuid,
    request: &CreatePaperReproductionRequest,
) -> Result<u64, ApiError> {
    let Some(previous_id) = request.supersedes_reproduction_id else {
        if memory.review.reproductions.values().any(|report| {
            report.evaluation_id == evaluation_id
                && report.reproducer_player_id == request.reproducer_player_id
                && report.supersedes_reproduction_id.is_none()
        }) {
            return Err(ApiError::conflict(
                "initial_reproduction_exists",
                "reproducer already has an initial immutable report for this evaluation",
            ));
        }
        return Ok(1);
    };
    let previous = memory
        .review
        .reproductions
        .get(&previous_id)
        .ok_or_else(|| {
            ApiError::not_found(
                "superseded_reproduction_not_found",
                "superseded reproduction does not exist",
            )
        })?;
    if previous.evaluation_id != evaluation_id
        || previous.reproducer_player_id != request.reproducer_player_id
        || memory
            .review
            .reproductions
            .values()
            .any(|report| report.supersedes_reproduction_id == Some(previous_id))
    {
        return Err(ApiError::conflict(
            "invalid_reproduction_supersession",
            "only the latest report by the same reproducer may be superseded",
        ));
    }
    previous
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("reproduction version overflow"))
}

async fn reproduction_version_postgres(
    tx: &mut Transaction<'_, Postgres>,
    evaluation_id: Uuid,
    request: &CreatePaperReproductionRequest,
) -> Result<u64, ApiError> {
    let Some(previous_id) = request.supersedes_reproduction_id else {
        let exists = sqlx::query_scalar::<_, bool>(
            "select exists(select 1 from hepta_paper_reproductions
              where evaluation_id=$1 and reproducer_player_id=$2
                and supersedes_reproduction_id is null)",
        )
        .bind(evaluation_id)
        .bind(request.reproducer_player_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if exists {
            return Err(ApiError::conflict(
                "initial_reproduction_exists",
                "reproducer already has an initial immutable report for this evaluation",
            ));
        }
        return Ok(1);
    };
    let row = sqlx::query(
        "select record_json from hepta_paper_reproductions
         where reproduction_id=$1 for share",
    )
    .bind(previous_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "superseded_reproduction_not_found",
            "superseded reproduction does not exist",
        )
    })?;
    let previous: PaperReproduction = decode_record(row.get("record_json"), "paper reproduction")?;
    if previous.evaluation_id != evaluation_id
        || previous.reproducer_player_id != request.reproducer_player_id
    {
        return Err(ApiError::conflict(
            "invalid_reproduction_supersession",
            "only the latest report by the same reproducer may be superseded",
        ));
    }
    previous
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("reproduction version overflow"))
}

async fn create_paper_reproduction(
    State(state): State<AppState>,
    Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperReproductionRequest>,
) -> Result<(StatusCode, Json<PaperReproduction>), ApiError> {
    const OPERATION: &str = "create_paper_reproduction_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/reproductions");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.reproducer_player_id {
        return Err(ApiError::forbidden(
            "reproducer_assertion_mismatch",
            "Consumer assertion must identify the reproducer",
        ));
    }
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let evaluation = next
            .review
            .evaluations
            .get(&evaluation_id)
            .filter(|evaluation| evaluation.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_evaluation_not_found", "evaluation does not exist")
            })?;
        let version = reproduction_version_memory(&next, evaluation_id, &request)?;
        let (report, frame) = prepare_reproduction(&context, &evaluation, &request, version, now)?;
        let (player, key) = active_player_key_memory(
            &next,
            request.reproducer_player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &player)?;
        verify_signature(
            &frame,
            &request.signature,
            &key,
            "reproduction_signature_failed",
            "reproduction signature verification failed",
        )?;
        if next
            .review
            .reproductions
            .insert(report.reproduction_id, report.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "paper_reproduction_exists",
                "reproduction_id already exists",
            ));
        }
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.reproduction.recorded.v1",
            paper_id,
            paper_id,
            report.version,
            json!({"reproduction_id":report.reproduction_id,"evaluation_id":evaluation_id,"status":report.status,"report_hash":report.report_hash}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &report,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(report)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let row = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where evaluation_id=$1 and paper_project_id=$2 for share",
    )
    .bind(evaluation_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_evaluation_not_found", "evaluation does not exist")
    })?;
    let evaluation: PaperEvaluation = decode_record(row.get("record_json"), "paper evaluation")?;
    let version = reproduction_version_postgres(&mut tx, evaluation_id, &request).await?;
    let (report, frame) = prepare_reproduction(&context, &evaluation, &request, version, now)?;
    let (player, key) = active_player_key_postgres(
        &mut tx,
        request.reproducer_player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &player)?;
    verify_signature(
        &frame,
        &request.signature,
        &key,
        "reproduction_signature_failed",
        "reproduction signature verification failed",
    )?;
    let result = sqlx::query(
        "insert into hepta_paper_reproductions (
            reproduction_id,evaluation_id,paper_project_id,reproducer_player_id,
            release_candidate_hash,paper_bundle_hash,tolerance_policy_hash,reproduced,
            supersedes_reproduction_id,report_hash,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13)",
    )
    .bind(report.reproduction_id)
    .bind(evaluation_id)
    .bind(paper_id)
    .bind(report.reproducer_player_id)
    .bind(&report.release_candidate_hash)
    .bind(&report.paper_bundle_hash)
    .bind(&report.tolerance_policy_hash)
    .bind(report.status == ReproductionStatus::Reproduced)
    .bind(report.supersedes_reproduction_id)
    .bind(&report.report_hash)
    .bind(i64::try_from(report.version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(
        serde_json::to_value(&report)
            .map_err(|error| ApiError::internal(format!("encode reproduction report: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_reproduction_exists",
                "reproduction identity or immutable supersession slot already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.reproduction.recorded.v1",
        paper_id,
        paper_id,
        report.version,
        json!({"reproduction_id":report.reproduction_id,"evaluation_id":evaluation_id,"status":report.status,"report_hash":report.report_hash}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(report.reproduction_id),
        StatusCode::CREATED,
        &report,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(report)))
}

fn prepare_appeal(
    context: &ReviewPaperContext,
    evaluation: &PaperEvaluation,
    request: &CreatePaperAppealRequest,
    now: DateTime<Utc>,
) -> Result<(PaperAppeal, Vec<u8>), ApiError> {
    for (field, value) in [
        (
            "release_candidate_hash",
            request.release_candidate_hash.as_str(),
        ),
        ("grounds_hash", request.grounds_hash.as_str()),
        (
            "evidence_manifest_hash",
            request.evidence_manifest_hash.as_str(),
        ),
    ] {
        validate_digest(field, value)?;
    }
    signed_time(request.signed_at_unix)?;
    if evaluation.paper_project_id != context.paper.paper_project_id
        || evaluation.release_candidate_hash != request.release_candidate_hash
        || context.release_candidate_hash != request.release_candidate_hash
    {
        return Err(ApiError::conflict(
            "appeal_release_mismatch",
            "Appeal must bind the exact evaluated frozen release",
        ));
    }
    if !context
        .team
        .members
        .iter()
        .any(|member| member.player_id == request.appellant_player_id)
    {
        return Err(ApiError::forbidden(
            "appellant_not_author",
            "only a human author on the frozen team may open an Appeal",
        ));
    }
    let signing = PaperAppealSigningV1 {
        schema: PAPER_APPEAL_V1.to_string(),
        appeal_id: request.appeal_id,
        evaluation_id: evaluation.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        appellant_player_id: request.appellant_player_id,
        grounds_hash: request.grounds_hash.clone(),
        evidence_manifest_hash: request.evidence_manifest_hash.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let frame = paper_appeal_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_appeal_contract", message))?;
    Ok((
        PaperAppeal {
            schema: PAPER_APPEAL_V1.to_string(),
            appeal_id: request.appeal_id,
            evaluation_id: evaluation.evaluation_id,
            paper_project_id: context.paper.paper_project_id,
            release_candidate_hash: request.release_candidate_hash.clone(),
            appellant_player_id: request.appellant_player_id,
            grounds_hash: request.grounds_hash.clone(),
            evidence_manifest_hash: request.evidence_manifest_hash.clone(),
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            version: 1,
            created_at: now,
        },
        frame,
    ))
}

async fn create_paper_appeal(
    State(state): State<AppState>,
    Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperAppealRequest>,
) -> Result<(StatusCode, Json<PaperAppeal>), ApiError> {
    const OPERATION: &str = "create_paper_appeal_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/appeals");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.appellant_player_id {
        return Err(ApiError::forbidden(
            "appellant_assertion_mismatch",
            "Consumer assertion must identify the appellant",
        ));
    }
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let evaluation = next
            .review
            .evaluations
            .get(&evaluation_id)
            .filter(|evaluation| evaluation.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_evaluation_not_found", "evaluation does not exist")
            })?;
        if next
            .review
            .appeals
            .values()
            .any(|appeal| appeal.evaluation_id == evaluation_id)
        {
            return Err(ApiError::conflict(
                "paper_appeal_exists",
                "evaluation already has an immutable Appeal",
            ));
        }
        let (appeal, frame) = prepare_appeal(&context, &evaluation, &request, now)?;
        let (player, key) = active_player_key_memory(
            &next,
            request.appellant_player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &player)?;
        verify_signature(
            &frame,
            &request.signature,
            &key,
            "appeal_signature_failed",
            "Appeal signature verification failed",
        )?;
        if next
            .review
            .appeals
            .insert(appeal.appeal_id, appeal.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "paper_appeal_exists",
                "appeal_id already exists",
            ));
        }
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.appeal.opened.v1",
            paper_id,
            paper_id,
            1,
            json!({"appeal_id":appeal.appeal_id,"evaluation_id":evaluation_id,"settlement_state":"challenged"}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &appeal,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(appeal)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let row = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where evaluation_id=$1 and paper_project_id=$2 for share",
    )
    .bind(evaluation_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_evaluation_not_found", "evaluation does not exist")
    })?;
    let evaluation: PaperEvaluation = decode_record(row.get("record_json"), "paper evaluation")?;
    let (appeal, frame) = prepare_appeal(&context, &evaluation, &request, now)?;
    let (player, key) = active_player_key_postgres(
        &mut tx,
        request.appellant_player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &player)?;
    verify_signature(
        &frame,
        &request.signature,
        &key,
        "appeal_signature_failed",
        "Appeal signature verification failed",
    )?;
    let result = sqlx::query(
        "insert into hepta_paper_appeals (
            appeal_id,evaluation_id,paper_project_id,appellant_player_id,
            release_candidate_hash,grounds_hash,evidence_manifest_hash,signature,
            version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,1,$9::jsonb,$10)",
    )
    .bind(appeal.appeal_id)
    .bind(evaluation_id)
    .bind(paper_id)
    .bind(appeal.appellant_player_id)
    .bind(&appeal.release_candidate_hash)
    .bind(&appeal.grounds_hash)
    .bind(&appeal.evidence_manifest_hash)
    .bind(&appeal.signature)
    .bind(
        serde_json::to_value(&appeal)
            .map_err(|error| ApiError::internal(format!("encode Appeal: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_appeal_exists",
                "evaluation or appeal_id already has an immutable Appeal",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.appeal.opened.v1",
        paper_id,
        paper_id,
        1,
        json!({"appeal_id":appeal.appeal_id,"evaluation_id":evaluation_id,"settlement_state":"challenged"}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(appeal.appeal_id),
        StatusCode::CREATED,
        &appeal,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(appeal)))
}

fn prepare_resolution(
    context: &ReviewPaperContext,
    appeal: &PaperAppeal,
    evaluation: &PaperEvaluation,
    superseding: Option<&PaperEvaluation>,
    request: &ResolvePaperAppealRequest,
    now: DateTime<Utc>,
) -> Result<(PaperAppealResolution, Vec<u8>), ApiError> {
    validate_digest("decision_hash", &request.decision_hash)?;
    signed_time(request.signed_at_unix)?;
    let authors: HashSet<_> = context
        .team
        .members
        .iter()
        .map(|member| member.player_id)
        .collect();
    if authors.contains(&request.resolver_player_id)
        || panel_members(evaluation).contains(&request.resolver_player_id)
        || superseding.is_some_and(|replacement| {
            panel_members(replacement).contains(&request.resolver_player_id)
        })
        || appeal.appellant_player_id == request.resolver_player_id
    {
        return Err(ApiError::forbidden(
            "appeal_resolver_not_independent",
            "resolver must be distinct from authors, appellant, evaluator, and reviewers",
        ));
    }
    match request.outcome {
        AppealOutcome::Upheld => {
            let replacement = superseding.ok_or_else(|| {
                ApiError::conflict(
                    "superseding_evaluation_required",
                    "upheld Appeal requires the immutable superseding evaluation",
                )
            })?;
            if request.superseding_evaluation_id != Some(replacement.evaluation_id)
                || replacement.supersedes_evaluation_id != Some(evaluation.evaluation_id)
                || replacement.paper_project_id != evaluation.paper_project_id
                || replacement.release_candidate_hash != evaluation.release_candidate_hash
                || replacement.paper_bundle_hash != evaluation.paper_bundle_hash
            {
                return Err(ApiError::conflict(
                    "invalid_superseding_evaluation",
                    "upheld Appeal must reference the exact same-release superseding evaluation",
                ));
            }
        }
        AppealOutcome::Denied => {
            if request.superseding_evaluation_id.is_some() || superseding.is_some() {
                return Err(ApiError::conflict(
                    "denied_appeal_cannot_supersede",
                    "denied Appeal must not reference a superseding evaluation",
                ));
            }
        }
    }
    let signing = PaperAppealResolutionSigningV1 {
        schema: PAPER_APPEAL_RESOLUTION_V1.to_string(),
        resolution_id: request.resolution_id,
        appeal_id: appeal.appeal_id,
        evaluation_id: evaluation.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        release_candidate_hash: appeal.release_candidate_hash.clone(),
        outcome: request.outcome.as_str().to_string(),
        superseding_evaluation_id: request.superseding_evaluation_id,
        decision_hash: request.decision_hash.clone(),
        resolver_player_id: request.resolver_player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let frame = paper_appeal_resolution_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_appeal_resolution_contract", message))?;
    Ok((
        PaperAppealResolution {
            schema: PAPER_APPEAL_RESOLUTION_V1.to_string(),
            resolution_id: request.resolution_id,
            appeal_id: appeal.appeal_id,
            evaluation_id: evaluation.evaluation_id,
            paper_project_id: context.paper.paper_project_id,
            release_candidate_hash: appeal.release_candidate_hash.clone(),
            outcome: request.outcome.clone(),
            superseding_evaluation_id: request.superseding_evaluation_id,
            decision_hash: request.decision_hash.clone(),
            resolver_player_id: request.resolver_player_id,
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            version: 1,
            created_at: now,
        },
        frame,
    ))
}

async fn resolve_paper_appeal(
    State(state): State<AppState>,
    Path((paper_id, appeal_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<ResolvePaperAppealRequest>,
) -> Result<(StatusCode, Json<PaperAppealResolution>), ApiError> {
    const OPERATION: &str = "resolve_paper_appeal_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/appeals/{appeal_id}/resolve");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.resolver_player_id {
        return Err(ApiError::forbidden(
            "resolver_assertion_mismatch",
            "Consumer assertion must identify the Appeal resolver",
        ));
    }
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let appeal = next
            .review
            .appeals
            .get(&appeal_id)
            .filter(|appeal| appeal.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_appeal_not_found", "Appeal does not exist")
            })?;
        if next
            .review
            .resolutions
            .values()
            .any(|resolution| resolution.appeal_id == appeal_id)
        {
            return Err(ApiError::conflict(
                "appeal_resolution_exists",
                "Appeal already has an immutable resolution",
            ));
        }
        let evaluation = next
            .review
            .evaluations
            .get(&appeal.evaluation_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("appealed evaluation record is missing"))?;
        let superseding = request
            .superseding_evaluation_id
            .and_then(|id| next.review.evaluations.get(&id));
        let (resolution, frame) =
            prepare_resolution(&context, &appeal, &evaluation, superseding, &request, now)?;
        let (player, key) = active_player_key_memory(
            &next,
            request.resolver_player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &player)?;
        verify_signature(
            &frame,
            &request.signature,
            &key,
            "appeal_resolution_signature_failed",
            "Appeal resolution signature verification failed",
        )?;
        if next
            .review
            .resolutions
            .insert(resolution.resolution_id, resolution.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "appeal_resolution_exists",
                "resolution_id already exists",
            ));
        }
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.appeal.resolved.v1",
            paper_id,
            paper_id,
            1,
            json!({"resolution_id":resolution.resolution_id,"appeal_id":appeal_id,"outcome":resolution.outcome,"settlement_state":"resolved"}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &resolution,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(resolution)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let row = sqlx::query(
        "select record_json from hepta_paper_appeals
         where appeal_id=$1 and paper_project_id=$2 for share",
    )
    .bind(appeal_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("paper_appeal_not_found", "Appeal does not exist"))?;
    let appeal: PaperAppeal = decode_record(row.get("record_json"), "paper Appeal")?;
    let eval_row = sqlx::query(
        "select record_json from hepta_paper_evaluations where evaluation_id=$1 for share",
    )
    .bind(appeal.evaluation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let evaluation: PaperEvaluation =
        decode_record(eval_row.get("record_json"), "paper evaluation")?;
    let superseding = if let Some(id) = request.superseding_evaluation_id {
        let row = sqlx::query(
            "select record_json from hepta_paper_evaluations where evaluation_id=$1 for share",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "superseding_evaluation_not_found",
                "superseding evaluation does not exist",
            )
        })?;
        Some(decode_record::<PaperEvaluation>(
            row.get("record_json"),
            "superseding evaluation",
        )?)
    } else {
        None
    };
    let (resolution, frame) = prepare_resolution(
        &context,
        &appeal,
        &evaluation,
        superseding.as_ref(),
        &request,
        now,
    )?;
    let (player, key) = active_player_key_postgres(
        &mut tx,
        request.resolver_player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &player)?;
    verify_signature(
        &frame,
        &request.signature,
        &key,
        "appeal_resolution_signature_failed",
        "Appeal resolution signature verification failed",
    )?;
    let result = sqlx::query(
        "insert into hepta_paper_appeal_resolutions (
            resolution_id,appeal_id,paper_project_id,resolver_player_id,outcome,
            superseding_evaluation_id,decision_hash,signature,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,1,$9::jsonb,$10)",
    )
    .bind(resolution.resolution_id)
    .bind(appeal_id)
    .bind(paper_id)
    .bind(resolution.resolver_player_id)
    .bind(resolution.outcome.as_str())
    .bind(resolution.superseding_evaluation_id)
    .bind(&resolution.decision_hash)
    .bind(&resolution.signature)
    .bind(
        serde_json::to_value(&resolution)
            .map_err(|error| ApiError::internal(format!("encode Appeal resolution: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "appeal_resolution_exists",
                "Appeal or resolution_id already has an immutable resolution",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.appeal.resolved.v1",
        paper_id,
        paper_id,
        1,
        json!({"resolution_id":resolution.resolution_id,"appeal_id":appeal_id,"outcome":resolution.outcome,"settlement_state":"resolved"}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(resolution.resolution_id),
        StatusCode::CREATED,
        &resolution,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(resolution)))
}

fn project_settlement_states(
    evaluations: &mut [PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) {
    for evaluation in evaluations {
        let appeal = appeals
            .iter()
            .find(|appeal| appeal.evaluation_id == evaluation.evaluation_id);
        evaluation.settlement_state = match appeal {
            Some(appeal)
                if resolutions
                    .iter()
                    .any(|resolution| resolution.appeal_id == appeal.appeal_id) =>
            {
                ReviewSettlementState::Resolved
            }
            Some(_) => ReviewSettlementState::Challenged,
            None => ReviewSettlementState::PendingFinality,
        };
    }
}

fn review_actor_allowed(
    player_id: Uuid,
    context: &ReviewPaperContext,
    model: &PaperReviewReadModel,
) -> bool {
    context
        .team
        .members
        .iter()
        .any(|member| member.player_id == player_id)
        || model.evaluations.iter().any(|evaluation| {
            evaluation.evaluator_player_id == player_id
                || evaluation
                    .reviewer_attestations
                    .iter()
                    .any(|review| review.reviewer_player_id == player_id)
        })
        || model
            .reproductions
            .iter()
            .any(|report| report.reproducer_player_id == player_id)
        || model
            .appeals
            .iter()
            .any(|appeal| appeal.appellant_player_id == player_id)
        || model
            .resolutions
            .iter()
            .any(|resolution| resolution.resolver_player_id == player_id)
}

async fn get_paper_review_state(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PaperReviewReadModel>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/review-state");
    let assertion =
        require_member_read_assertion(&headers, &state, "get_paper_review_state_v1", &path)?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let context = review_context_memory(&memory, paper_id)?;
        let mut model = PaperReviewReadModel {
            contribution_ledgers: memory
                .review
                .contribution_ledgers
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
            evaluations: memory
                .review
                .evaluations
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
            reproductions: memory
                .review
                .reproductions
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
            appeals: memory
                .review
                .appeals
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
            resolutions: memory
                .review
                .resolutions
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
            raid_scores: memory
                .review
                .raid_scores
                .values()
                .filter(|record| record.paper_project_id == paper_id)
                .cloned()
                .collect(),
        };
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden("review_actor_not_found", "asserted review actor is unknown")
        })?;
        assert_player_identity(&assertion, player)?;
        if !review_actor_allowed(assertion.player_id, &context, &model) {
            return Err(ApiError::forbidden(
                "review_state_access_denied",
                "review state is limited to authors and signed review participants",
            ));
        }
        project_settlement_states(&mut model.evaluations, &model.appeals, &model.resolutions);
        sort_review_model(&mut model);
        return Ok(Json(model));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let mut model = PaperReviewReadModel {
        contribution_ledgers: load_review_records(
            &mut tx,
            "hepta_paper_contribution_ledgers",
            paper_id,
            "contribution ledger",
        )
        .await?,
        evaluations: load_review_records(
            &mut tx,
            "hepta_paper_evaluations",
            paper_id,
            "paper evaluation",
        )
        .await?,
        reproductions: load_review_records(
            &mut tx,
            "hepta_paper_reproductions",
            paper_id,
            "paper reproduction",
        )
        .await?,
        appeals: load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal")
            .await?,
        resolutions: load_review_records(
            &mut tx,
            "hepta_paper_appeal_resolutions",
            paper_id,
            "Appeal resolution",
        )
        .await?,
        raid_scores: load_review_records(
            &mut tx,
            "hepta_paper_raid_scores",
            paper_id,
            "raid score",
        )
        .await?,
    };
    let player_row = sqlx::query("select record_json from hepta_human_players where player_id=$1")
        .bind(assertion.player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::forbidden("review_actor_not_found", "asserted review actor is unknown")
        })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    if !review_actor_allowed(assertion.player_id, &context, &model) {
        return Err(ApiError::forbidden(
            "review_state_access_denied",
            "review state is limited to authors and signed review participants",
        ));
    }
    project_settlement_states(&mut model.evaluations, &model.appeals, &model.resolutions);
    sort_review_model(&mut model);
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(model))
}

async fn load_review_records<T: serde::de::DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    table: &'static str,
    paper_id: Uuid,
    kind: &'static str,
) -> Result<Vec<T>, ApiError> {
    let sql =
        format!("select record_json from {table} where paper_project_id=$1 order by created_at");
    let rows = sqlx::query(&sql)
        .bind(paper_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    rows.into_iter()
        .map(|row| decode_record(row.get("record_json"), kind))
        .collect()
}

fn sort_review_model(model: &mut PaperReviewReadModel) {
    model
        .contribution_ledgers
        .sort_by_key(|record| record.contribution_ledger_id);
    model.evaluations.sort_by_key(|record| record.evaluation_id);
    model
        .reproductions
        .sort_by_key(|record| record.reproduction_id);
    model.appeals.sort_by_key(|record| record.appeal_id);
    model.resolutions.sort_by_key(|record| record.resolution_id);
    model.raid_scores.sort_by_key(|record| record.raid_score_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_score_hard_gate_overrides_a_perfect_component_total() {
        let components = PaperScoreComponents {
            method_rigor_bps: 2_500,
            experiment_statistics_bps: 1_500,
            reproducibility_bps: 1_500,
            evidence_citations_bps: 1_500,
            value_originality_bps: 1_500,
            argument_expression_bps: 1_000,
            ethics_transparency_bps: 500,
        };
        assert_eq!(
            components.validate_and_total().expect("valid score"),
            10_000
        );

        let gates = PaperHardGates {
            citations_and_data_authentic: true,
            failed_runs_disclosed: false,
            all_authors_consented: true,
            core_claims_have_evidence: true,
            artifact_lineage_complete: true,
            license_ethics_coi_complete: true,
        };
        assert!(!gates.eligible());
    }

    #[test]
    fn paper_score_rejects_a_component_above_its_frozen_weight() {
        let components = PaperScoreComponents {
            method_rigor_bps: 2_501,
            experiment_statistics_bps: 0,
            reproducibility_bps: 0,
            evidence_citations_bps: 0,
            value_originality_bps: 0,
            argument_expression_bps: 0,
            ethics_transparency_bps: 0,
        };
        assert!(components.validate_and_total().is_err());
    }
}
