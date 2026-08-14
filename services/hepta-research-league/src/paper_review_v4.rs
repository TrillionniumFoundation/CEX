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
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use uuid::Uuid;

use super::collaboration_v3::{insert_room_event_postgres, push_room_event_memory};
use super::*;
use crate::paper_chain_finality_v1::{PaperChainFinalityProjectionV2, PaperChainFinalityStatusV1};
use crate::paper_raid_contracts::{
    canonical_json_sha256, frozen_review_authority_hash, paper_appeal_resolution_signing_bytes,
    paper_appeal_signing_bytes, paper_evaluation_signing_bytes, paper_reproduction_signing_bytes,
    paper_review_attestation_signing_bytes, sha256_digest, verify_frozen_review_authority,
    FrozenReviewAuthorityV1, FrozenReviewExecutionPolicyV1, FrozenReviewObjectV1,
    PaperAppealResolutionSigningV1, PaperAppealSigningV1, PaperEvaluationSigningV1,
    PaperReproductionSigningV1, PaperReviewAttestationSigningV1, FROZEN_REVIEW_AUTHORITY_V1,
    PAPER_APPEAL_RESOLUTION_V1, PAPER_APPEAL_V1, PAPER_EVALUATION_V1, PAPER_REPRODUCTION_V1,
    PAPER_REVIEW_ATTESTATION_V1, REVIEW_OBJECT_DOWNLOAD_PATH_V1,
};

pub const PAPER_REVIEW_PROTOCOL_V4: &str = "hepta.paper_raid.review.v4";
pub const CONTRIBUTION_LEDGER_SCHEMA_V1: &str = "hepta.paper_raid.contribution_ledger.v1";
pub const PAPER_SCORE_SCHEMA_V1: &str = "hepta.paper_raid.paper_score.v1";
pub const RAID_SCORE_SCHEMA_V1: &str = "hepta.paper_raid.raid_score.v1";
pub const TOLERANCE_POLICY_SCHEMA_V1: &str = "hepta.paper_raid.tolerance_policy.v1";
pub const REVIEW_ASSIGNMENT_SCHEMA_V1: &str = "hepta.paper_raid.review_assignment.v1";
pub const EVALUATION_DRAFT_SCHEMA_V1: &str = "hepta.paper_raid.evaluation_draft.v1";
pub const EVALUATION_DRAFT_SCHEMA_V2: &str = "hepta.paper_raid.evaluation_draft.v2";
pub const EVALUATION_DRAFT_ATTESTATION_SCHEMA_V1: &str =
    "hepta.paper_raid.evaluation_draft_attestation.v1";
pub const EVALUATION_DRAFT_QUORUM_SCHEMA_V1: &str = "hepta.paper_raid.evaluation_draft_quorum.v1";
const REVIEW_ASSIGNMENT_TTL_HOURS: i64 = 24;
const EVALUATION_DRAFT_LEASE_HOURS: i64 = 24;
const ACCEPTED_ARTIFACT_MILESTONE_XP: u64 = 100;
const ACCEPTED_REVIEW_MILESTONE_XP: u64 = 150;
const ACCEPTED_AUTHOR_RAID_BASE_XP: u64 = 300;

pub(crate) async fn verify_legacy_evaluation_panel_lifecycle_catalog(
    pool: &sqlx::PgPool,
) -> Result<(), String> {
    let trigger_catalog = sqlx::query(
        "select count(*)::bigint as total,
                count(*) filter (
                  where t.tgrelid='public.hepta_paper_review_assignments'::regclass
                    and p.proname='hepta_guard_review_assignment_draft_lifecycle_v1'
                    and n.nspname='public'
                    and t.tgtype=19 and t.tgenabled='A'
                )::bigint as exact
         from pg_trigger t
         join pg_proc p on p.oid=t.tgfoid
         join pg_namespace n on n.oid=p.pronamespace
         where t.tgname='hepta_review_assignment_draft_lifecycle_guard'
           and not t.tgisinternal",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect legacy evaluation panel lifecycle trigger: {error}"))?;
    if trigger_catalog.get::<i64, _>("total") != 1 || trigger_catalog.get::<i64, _>("exact") != 1 {
        return Err(
            "legacy evaluation frozen-panel lifecycle trigger is not globally unique, exact, and ALWAYS"
                .to_string(),
        );
    }
    let stale_draft_only_fk: i64 = sqlx::query_scalar(
        "select count(*)::bigint from pg_constraint
         where conname='hepta_paper_review_assignments_pinned_evaluation_fkey'",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect legacy evaluation pinned authority constraint: {error}"))?;
    if stale_draft_only_fk != 0 {
        return Err(
            "legacy evaluation panel lifecycle remains constrained to draft-only authority"
                .to_string(),
        );
    }
    let function_catalog = sqlx::query(
        "select p.prosrc,p.prosecdef,p.provolatile::text as volatility,
                l.lanname,coalesce(array_to_string(p.proconfig,','),'') as config
         from pg_proc p
         join pg_namespace n on n.oid=p.pronamespace
         join pg_language l on l.oid=p.prolang
         where n.nspname='public'
           and p.proname='hepta_guard_review_assignment_draft_lifecycle_v1'
           and p.pronargs=0 and p.prokind='f'",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("inspect legacy evaluation panel lifecycle function: {error}"))?;
    if function_catalog.get::<bool, _>("prosecdef")
        || function_catalog.get::<String, _>("volatility") != "v"
        || function_catalog.get::<String, _>("lanname") != "plpgsql"
        || function_catalog.get::<String, _>("config") != "search_path=pg_catalog, public"
    {
        return Err(
            "legacy evaluation frozen-panel lifecycle function metadata is non-canonical"
                .to_string(),
        );
    }
    let migration =
        include_str!("../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql");
    let body_start = migration
        .find("as $function$\n")
        .map(|index| index + "as $function$\n".len())
        .ok_or_else(|| "0051 canonical lifecycle function body start is missing".to_string())?;
    let body_end = migration[body_start..]
        .find("\n$function$;")
        .map(|index| body_start + index)
        .ok_or_else(|| "0051 canonical lifecycle function body end is missing".to_string())?;
    let normalize = |value: &str| {
        value
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let expected = normalize(&migration[body_start..body_end]);
    let actual = normalize(&function_catalog.get::<String, _>("prosrc"));
    if actual != expected {
        return Err(
            "legacy evaluation frozen-panel lifecycle function body is not the exact 0051 authority"
                .to_string(),
        );
    }
    Ok(())
}

/// Read the exact microsecond-precision value PostgreSQL will persist so an
/// immutable JSON projection and its relational `timestamptz` remain equal.
async fn postgres_contribution_ledger_now(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<DateTime<Utc>, ApiError> {
    sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::database)
}

#[derive(Clone, Default)]
pub(crate) struct ReviewMemory {
    pub(super) assignments: HashMap<Uuid, ReviewAssignment>,
    pub(super) evaluation_drafts: HashMap<Uuid, PaperEvaluationDraft>,
    pub(super) draft_attestations: HashMap<Uuid, EvaluationDraftAttestation>,
    pub(super) contribution_ledgers: HashMap<Uuid, ContributionLedger>,
    pub(crate) evaluations: HashMap<Uuid, PaperEvaluation>,
    pub(crate) reproductions: HashMap<Uuid, PaperReproduction>,
    pub(crate) appeals: HashMap<Uuid, PaperAppeal>,
    pub(crate) resolutions: HashMap<Uuid, PaperAppealResolution>,
    raid_scores: HashMap<Uuid, RaidScore>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAssignmentSlot {
    Evaluator,
    #[serde(rename = "reviewer_1")]
    Reviewer1,
    #[serde(rename = "reviewer_2")]
    Reviewer2,
    Reproducer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAssignmentStatus {
    Claimed,
    Pinned,
    Consumed,
    Expired,
}

impl ReviewAssignmentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Pinned => "pinned",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
        }
    }
}

impl ReviewAssignmentSlot {
    fn as_str(self) -> &'static str {
        match self {
            Self::Evaluator => "evaluator",
            Self::Reviewer1 => "reviewer_1",
            Self::Reviewer2 => "reviewer_2",
            Self::Reproducer => "reproducer",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Evaluator => 0,
            Self::Reviewer1 => 1,
            Self::Reviewer2 => 2,
            Self::Reproducer => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAssignment {
    pub schema: String,
    pub assignment_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub player_id: Uuid,
    pub review_round: u64,
    pub slot: ReviewAssignmentSlot,
    pub pinned_evaluation_id: Option<Uuid>,
    pub status: ReviewAssignmentStatus,
    pub version: u64,
    pub claimed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimReviewAssignmentRequest {
    pub assignment_id: Uuid,
    pub player_id: Uuid,
    pub review_round: u64,
    pub slot: ReviewAssignmentSlot,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAssignmentVacancy {
    pub review_round: u64,
    pub slot: ReviewAssignmentSlot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueItem {
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub challenge_id: Uuid,
    pub title: String,
    pub abstract_text: String,
    pub target_format: String,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub author_count: u32,
    pub submitted_at: DateTime<Utc>,
    pub my_assignments: Vec<ReviewAssignment>,
    pub open_slots: Vec<ReviewAssignmentVacancy>,
}

/// Assignment-scoped extension of the existing frozen JointPaperSubmission.
///
/// `submission` is flattened so existing review-bundle consumers continue to
/// see the exact same top-level PaperBundle fields. The additive fields let a
/// reviewer discover the evaluator draft and let the assigned reproducer
/// discover the finalized evaluation without joining the author Paper Room.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperReviewBundleV1 {
    #[serde(flatten)]
    pub submission: JointPaperSubmission,
    pub my_assignments: Vec<ReviewAssignment>,
    pub frozen_review_authority: FrozenReviewAuthorityV1,
    pub evaluation_quorum: Option<EvaluationDraftQuorum>,
    pub evaluation: Option<PaperEvaluation>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationDraftStatus {
    Open,
    Finalized,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperEvaluationDraft {
    pub schema: String,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub review_round: u64,
    pub supersedes_evaluation_id: Option<Uuid>,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub tolerance_policy: TolerancePolicy,
    pub tolerance_policy_hash: String,
    pub reference_metrics_micros: BTreeMap<String, i64>,
    pub paper_score: PaperScore,
    pub evaluator_player_id: Uuid,
    pub evaluator_signing_key_id: String,
    pub evaluator_signing_public_key: String,
    pub evaluator_signing_public_key_hash: String,
    pub evaluator_coi_attestation_hash: String,
    pub evaluator_signed_at_unix: i64,
    pub evaluator_signature: String,
    pub evaluation_signing_hash: String,
    pub draft_hash: String,
    pub status: EvaluationDraftStatus,
    pub version: u64,
    pub finalized_evaluation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperEvaluationDraftRequest {
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
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitEvaluationDraftAttestationRequest {
    pub attestation_id: Uuid,
    pub draft_hash: String,
    pub reviewer_player_id: Uuid,
    pub verdict: PanelVerdict,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub coi_attestation_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationDraftAttestation {
    pub schema: String,
    pub attestation_id: Uuid,
    pub evaluation_id: Uuid,
    pub paper_project_id: Uuid,
    pub submission_id: Uuid,
    pub review_round: u64,
    pub slot: ReviewAssignmentSlot,
    pub draft_hash: String,
    pub attestation: ReviewAttestation,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalizePaperEvaluationDraftRequest {
    pub expected_draft_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationDraftQuorum {
    pub schema: String,
    pub draft: PaperEvaluationDraft,
    pub attestations: Vec<EvaluationDraftAttestation>,
    pub required_slots: Vec<ReviewAssignmentSlot>,
    pub missing_slots: Vec<ReviewAssignmentSlot>,
    pub assignments_active: bool,
    pub ready_to_finalize: bool,
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
    pub finality: ConsumerPaperFinalityV2,
    pub assignments: Vec<ReviewAssignment>,
    /// Open and expired draft authority needed to interpret pinned assignment
    /// lifecycles. Finalized drafts are omitted because their assignments are
    /// consumed against the immutable `evaluations` authority below.
    pub evaluation_drafts: Vec<PaperEvaluationDraft>,
    pub contribution_ledgers: Vec<ContributionLedger>,
    pub evaluations: Vec<PaperEvaluation>,
    pub reproductions: Vec<PaperReproduction>,
    pub appeals: Vec<PaperAppeal>,
    pub resolutions: Vec<PaperAppealResolution>,
    pub raid_scores: Vec<RaidScore>,
}

pub const CONSUMER_PAPER_FINALITY_SCHEMA_V2: &str = "hepta.paper_raid.consumer_finality.v2";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsumerPaperFinalityV2 {
    pub schema: String,
    pub status: PaperChainFinalityStatusV1,
    pub effective_evaluation_id: Option<Uuid>,
    pub effective_reproduction_id: Option<Uuid>,
    pub effective_appeal_resolution_id: Option<Uuid>,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub score_eligible: bool,
    pub economic_eligible: bool,
    pub verified_at: Option<DateTime<Utc>>,
}

impl ConsumerPaperFinalityV2 {
    fn pending() -> Self {
        Self {
            schema: CONSUMER_PAPER_FINALITY_SCHEMA_V2.to_string(),
            status: PaperChainFinalityStatusV1::PendingFinality,
            effective_evaluation_id: None,
            effective_reproduction_id: None,
            effective_appeal_resolution_id: None,
            ranking_eligible: false,
            reward_eligible: false,
            score_eligible: false,
            economic_eligible: false,
            verified_at: None,
        }
    }

    fn from_projection(projection: PaperChainFinalityProjectionV2) -> Self {
        Self {
            schema: CONSUMER_PAPER_FINALITY_SCHEMA_V2.to_string(),
            status: projection.status,
            effective_evaluation_id: Some(projection.evaluation_id),
            effective_reproduction_id: Some(projection.reproduction_id),
            effective_appeal_resolution_id: projection.appeal_resolution_id,
            ranking_eligible: projection.ranking_eligible,
            reward_eligible: projection.reward_eligible,
            score_eligible: projection.score_eligible,
            economic_eligible: projection.economic_eligible,
            verified_at: Some(projection.verified_at),
        }
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/v2/hepta/review-queue", get(get_review_queue))
        .route(
            "/v2/hepta/papers/:paper_id/review-assignments",
            post(claim_review_assignment),
        )
        .route(
            "/v2/hepta/papers/:paper_id/review-bundle",
            get(get_paper_review_bundle),
        )
        .route(
            "/v2/hepta/papers/:paper_id/contribution-ledgers",
            post(create_contribution_ledger),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluations",
            post(create_paper_evaluation),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluation-drafts",
            post(create_paper_evaluation_draft),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id",
            get(get_paper_evaluation_draft),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id/attestations",
            post(submit_evaluation_draft_attestation),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evaluation-drafts/:evaluation_id/finalize",
            post(finalize_paper_evaluation_draft),
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
struct ReviewAccessContext {
    paper: PaperProject,
    team: ResearchTeam,
}

#[derive(Clone)]
struct ReviewPaperContext {
    paper: PaperProject,
    team: ResearchTeam,
    release_candidate: crate::paper_raid_contracts::PaperReleaseCandidateV2,
    release_candidate_hash: String,
}

fn review_access_context_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
) -> Result<ReviewAccessContext, ApiError> {
    let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let team = memory
        .teams
        .get(&paper.team_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
    Ok(ReviewAccessContext { paper, team })
}

fn review_context_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
) -> Result<ReviewPaperContext, ApiError> {
    let access = review_access_context_memory(memory, paper_id)?;
    let paper = access.paper;
    let team = access.team;
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

async fn review_access_context_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<ReviewAccessContext, ApiError> {
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
    Ok(ReviewAccessContext { paper, team })
}

async fn review_context_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<ReviewPaperContext, ApiError> {
    let access = review_access_context_postgres(tx, paper_id).await?;
    let paper = access.paper;
    let team = access.team;
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

fn active_registered_player_memory<'a>(
    memory: &'a PaperRaidMemory,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<&'a HumanPlayer, ApiError> {
    let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
        ApiError::forbidden(
            "human_player_not_registered",
            "review access requires a registered human player",
        )
    })?;
    assert_player_identity(assertion, player)?;
    if player.status != HumanPlayerStatus::Active {
        return Err(ApiError::forbidden(
            "human_player_not_active",
            "review access requires an active human player",
        ));
    }
    Ok(player)
}

async fn active_registered_player_postgres(
    tx: &mut Transaction<'_, Postgres>,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<HumanPlayer, ApiError> {
    let row =
        sqlx::query("select record_json from hepta_human_players where player_id=$1 for share")
            .bind(assertion.player_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::forbidden(
                    "human_player_not_registered",
                    "review access requires a registered human player",
                )
            })?;
    let player: HumanPlayer = decode_record(row.get("record_json"), "human player")?;
    assert_player_identity(assertion, &player)?;
    if player.status != HumanPlayerStatus::Active {
        return Err(ApiError::forbidden(
            "human_player_not_active",
            "review access requires an active human player",
        ));
    }
    Ok(player)
}

pub(super) fn review_assignment_active(assignment: &ReviewAssignment, now: DateTime<Utc>) -> bool {
    match assignment.status {
        ReviewAssignmentStatus::Claimed => assignment.expires_at > now,
        ReviewAssignmentStatus::Pinned => true,
        ReviewAssignmentStatus::Consumed | ReviewAssignmentStatus::Expired => false,
    }
}

fn latest_review_evaluation_for_submission(
    paper_id: Uuid,
    submission_id: Uuid,
    evaluations: &[PaperEvaluation],
) -> Option<&PaperEvaluation> {
    evaluations
        .iter()
        .filter(|evaluation| {
            evaluation.paper_project_id == paper_id && evaluation.submission_id == submission_id
        })
        .max_by_key(|evaluation| (evaluation.version, evaluation.evaluation_id))
}

fn evaluation_has_open_appeal(
    evaluation_id: Uuid,
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> bool {
    appeals.iter().any(|appeal| {
        appeal.evaluation_id == evaluation_id
            && !resolutions
                .iter()
                .any(|resolution| resolution.appeal_id == appeal.appeal_id)
    })
}

fn activated_evaluation_at(
    paper_id: Uuid,
    evaluation_id: Uuid,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Option<DateTime<Utc>> {
    activated_evaluation_at_ignoring_prepared_child(
        paper_id,
        evaluation_id,
        evaluations,
        appeals,
        resolutions,
        None,
    )
}

fn activated_evaluation_at_ignoring_prepared_child(
    paper_id: Uuid,
    evaluation_id: Uuid,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
    prepared_child: Option<&PaperEvaluation>,
) -> Option<DateTime<Utc>> {
    let mut by_id = HashMap::new();
    for evaluation in evaluations
        .iter()
        .filter(|evaluation| evaluation.paper_project_id == paper_id)
    {
        if by_id.insert(evaluation.evaluation_id, evaluation).is_some() {
            return None;
        }
    }

    let mut reverse_lineage = Vec::new();
    let mut visited = HashSet::new();
    let mut current = *by_id.get(&evaluation_id)?;
    let ignored_prepared_child_id = match prepared_child {
        Some(child)
            if child.evaluation_id != current.evaluation_id
                && child.paper_project_id == current.paper_project_id
                && child.submission_id == current.submission_id
                && child.supersedes_evaluation_id == Some(current.evaluation_id) =>
        {
            Some(child.evaluation_id)
        }
        Some(_) => return None,
        None => None,
    };
    loop {
        if !visited.insert(current.evaluation_id) {
            return None;
        }
        reverse_lineage.push(current);
        let Some(parent_id) = current.supersedes_evaluation_id else {
            break;
        };
        current = *by_id.get(&parent_id)?;
    }
    reverse_lineage.reverse();

    let root = *reverse_lineage.first()?;
    if root.version != 1
        || root.supersedes_evaluation_id.is_some()
        || panel_members(root).len() != 3
    {
        return None;
    }
    if evaluations.iter().any(|candidate| {
        candidate.paper_project_id == paper_id
            && candidate.submission_id == root.submission_id
            && !visited.contains(&candidate.evaluation_id)
            && Some(candidate.evaluation_id) != ignored_prepared_child_id
    }) {
        return None;
    }
    let mut activated_at = root.created_at;
    let mut activation_resolution_ids = HashSet::new();
    for pair in reverse_lineage.windows(2) {
        let parent = pair[0];
        let child = pair[1];
        let parent_panel = panel_members(parent);
        let child_panel = panel_members(child);
        if child.supersedes_evaluation_id != Some(parent.evaluation_id)
            || child.submission_id != parent.submission_id
            || child.release_candidate_hash != parent.release_candidate_hash
            || child.paper_bundle_hash != parent.paper_bundle_hash
            || child.version != parent.version.checked_add(1)?
            || parent_panel.len() != 3
            || child_panel.len() != 3
            || !parent_panel.is_disjoint(&child_panel)
        {
            return None;
        }

        let mut matching_appeals = appeals.iter().filter(|appeal| {
            appeal.paper_project_id == paper_id && appeal.evaluation_id == parent.evaluation_id
        });
        let appeal = matching_appeals.next()?;
        if matching_appeals.next().is_some()
            || appeal.release_candidate_hash != parent.release_candidate_hash
            || appeal.created_at < activated_at
            || child.created_at < appeal.created_at
        {
            return None;
        }

        let mut matching_resolutions = resolutions.iter().filter(|resolution| {
            resolution.paper_project_id == paper_id && resolution.appeal_id == appeal.appeal_id
        });
        let resolution = matching_resolutions.next()?;
        if matching_resolutions.next().is_some()
            || resolution.evaluation_id != parent.evaluation_id
            || resolution.release_candidate_hash != parent.release_candidate_hash
            || resolution.outcome != AppealOutcome::Upheld
            || resolution.superseding_evaluation_id != Some(child.evaluation_id)
            || resolution.created_at < appeal.created_at
            || resolution.created_at < child.created_at
        {
            return None;
        }
        activation_resolution_ids.insert(resolution.resolution_id);
        activated_at = resolution.created_at;
    }
    if resolutions.iter().any(|resolution| {
        resolution.paper_project_id == paper_id
            && resolution
                .superseding_evaluation_id
                .is_some_and(|child_id| visited.contains(&child_id))
            && !activation_resolution_ids.contains(&resolution.resolution_id)
    }) {
        return None;
    }
    Some(activated_at)
}

fn ensure_appealed_evaluation_activated_for_resolution(
    evaluation: &PaperEvaluation,
    prepared_child: Option<&PaperEvaluation>,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Result<(), ApiError> {
    activated_evaluation_at_ignoring_prepared_child(
        evaluation.paper_project_id,
        evaluation.evaluation_id,
        evaluations,
        appeals,
        resolutions,
        prepared_child,
    )
    .map(|_| ())
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_evaluation_not_activated",
            "the appealed evaluation requires an exact active lineage; only its unique direct prepared replacement may remain pending activation",
        )
    })
}

fn ensure_evaluation_activated(
    evaluation: &PaperEvaluation,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Result<(), ApiError> {
    activated_evaluation_at(
        evaluation.paper_project_id,
        evaluation.evaluation_id,
        evaluations,
        appeals,
        resolutions,
    )
    .map(|_| ())
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_evaluation_not_activated",
            "the evaluation requires an exact root-to-leaf upheld Appeal lineage before downstream mutation",
        )
    })
}

struct HistoricalEvaluationAuthority<'a> {
    evaluation: &'a PaperEvaluation,
    activated_at: DateTime<Utc>,
    mutation_closed_at: Option<DateTime<Utc>>,
}

fn historical_evaluation_authority<'a>(
    effective_evaluation: &PaperEvaluation,
    historical_evaluation_id: Uuid,
    evaluations: &'a [PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Option<HistoricalEvaluationAuthority<'a>> {
    activated_evaluation_at(
        effective_evaluation.paper_project_id,
        effective_evaluation.evaluation_id,
        evaluations,
        appeals,
        resolutions,
    )?;
    let by_id = evaluations
        .iter()
        .filter(|candidate| candidate.paper_project_id == effective_evaluation.paper_project_id)
        .map(|candidate| (candidate.evaluation_id, candidate))
        .collect::<HashMap<_, _>>();
    let mut current = *by_id.get(&effective_evaluation.evaluation_id)?;
    let mut direct_child = None;
    loop {
        if current.evaluation_id == historical_evaluation_id {
            let activated_at = if let Some(parent_id) = current.supersedes_evaluation_id {
                let parent = *by_id.get(&parent_id)?;
                let mut matching_appeals = appeals.iter().filter(|appeal| {
                    appeal.paper_project_id == effective_evaluation.paper_project_id
                        && appeal.evaluation_id == parent.evaluation_id
                });
                let appeal = matching_appeals.next()?;
                if matching_appeals.next().is_some() {
                    return None;
                }
                let mut matching_resolutions = resolutions.iter().filter(|resolution| {
                    resolution.paper_project_id == effective_evaluation.paper_project_id
                        && resolution.appeal_id == appeal.appeal_id
                });
                let resolution = matching_resolutions.next()?;
                if matching_resolutions.next().is_some()
                    || resolution.evaluation_id != parent.evaluation_id
                    || resolution.outcome != AppealOutcome::Upheld
                    || resolution.superseding_evaluation_id != Some(current.evaluation_id)
                {
                    return None;
                }
                resolution.created_at
            } else {
                current.created_at
            };
            return Some(HistoricalEvaluationAuthority {
                evaluation: current,
                activated_at,
                mutation_closed_at: direct_child.map(|child: &PaperEvaluation| child.created_at),
            });
        }
        direct_child = Some(current);
        current = *by_id.get(&current.supersedes_evaluation_id?)?;
    }
}

fn validate_reproduction_activation(
    effective_evaluation: &PaperEvaluation,
    reproduction: &PaperReproduction,
    evaluations: &[PaperEvaluation],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Result<(), ApiError> {
    if reproduction.paper_project_id != effective_evaluation.paper_project_id
        || !evaluations.iter().any(|candidate| {
            candidate.paper_project_id == effective_evaluation.paper_project_id
                && candidate.evaluation_id == reproduction.evaluation_id
        })
    {
        return Err(ApiError::conflict(
            "paper_finality_reproduction_lineage_missing",
            "Paper reproduction references an evaluation outside the exact authority",
        ));
    }
    let authority = historical_evaluation_authority(
        effective_evaluation,
        reproduction.evaluation_id,
        evaluations,
        appeals,
        resolutions,
    )
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_finality_reproduction_before_activation",
            "Paper reproduction belongs to an evaluation outside the exact activated lineage",
        )
    })?;
    if reproduction.release_candidate_hash != authority.evaluation.release_candidate_hash
        || reproduction.paper_bundle_hash != authority.evaluation.paper_bundle_hash
        || reproduction.tolerance_policy_hash != authority.evaluation.tolerance_policy_hash
    {
        return Err(ApiError::conflict(
            "paper_finality_reproduction_authority_mismatch",
            "Paper reproduction scientific authority does not match its exact evaluation",
        ));
    }
    if reproduction.created_at < authority.activated_at {
        return Err(ApiError::conflict(
            "paper_finality_reproduction_before_activation",
            "Paper reproduction predates its evaluation's exact upheld Appeal activation",
        ));
    }
    if authority
        .mutation_closed_at
        .is_some_and(|closed_at| reproduction.created_at >= closed_at)
    {
        return Err(ApiError::conflict(
            "paper_finality_historical_reproduction_after_mutation_closed",
            "Historical evaluation reproduction must predate preparation of its direct replacement",
        ));
    }
    Ok(())
}

pub(crate) fn effective_finality_resolution_id(
    evaluation: &PaperEvaluation,
    evaluations: &[PaperEvaluation],
    reproductions: &[PaperReproduction],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
) -> Result<Option<Uuid>, ApiError> {
    let activated_at = activated_evaluation_at(
        evaluation.paper_project_id,
        evaluation.evaluation_id,
        evaluations,
        appeals,
        resolutions,
    )
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_finality_evaluation_not_activated",
            "effective evaluation is not the leaf of an exact root-to-leaf upheld Appeal lineage",
        )
    })?;
    for reproduction in reproductions
        .iter()
        .filter(|record| record.paper_project_id == evaluation.paper_project_id)
    {
        validate_reproduction_activation(
            evaluation,
            reproduction,
            evaluations,
            appeals,
            resolutions,
        )?;
    }
    if evaluation.created_at > activated_at {
        return Err(ApiError::conflict(
            "paper_finality_evaluation_activation_regressed",
            "effective evaluation activation predates its immutable evaluation record",
        ));
    }
    let exact_appeal = |evaluation_id: Uuid| -> Result<Option<&PaperAppeal>, ApiError> {
        let mut matching = appeals.iter().filter(|appeal| {
            appeal.paper_project_id == evaluation.paper_project_id
                && appeal.evaluation_id == evaluation_id
        });
        let appeal = matching.next();
        if matching.next().is_some() {
            return Err(ApiError::conflict(
                "paper_finality_appeal_binding_ambiguous",
                "effective evaluation has more than one Appeal binding",
            ));
        }
        Ok(appeal)
    };
    let exact_resolution = |appeal_id: Uuid| -> Result<Option<&PaperAppealResolution>, ApiError> {
        let mut matching = resolutions.iter().filter(|resolution| {
            resolution.paper_project_id == evaluation.paper_project_id
                && resolution.appeal_id == appeal_id
        });
        let resolution = matching.next();
        if matching.next().is_some() {
            return Err(ApiError::conflict(
                "paper_finality_resolution_binding_ambiguous",
                "effective Appeal has more than one resolution binding",
            ));
        }
        Ok(resolution)
    };

    if let Some(appeal) = exact_appeal(evaluation.evaluation_id)? {
        let resolution = exact_resolution(appeal.appeal_id)?.ok_or_else(|| {
            ApiError::conflict(
                "paper_chain_finality_open_appeal",
                "an unresolved Appeal holds Paper Chain finality",
            )
        })?;
        if appeal.release_candidate_hash != evaluation.release_candidate_hash
            || resolution.evaluation_id != evaluation.evaluation_id
            || resolution.release_candidate_hash != evaluation.release_candidate_hash
            || resolution.outcome != AppealOutcome::Denied
            || resolution.superseding_evaluation_id.is_some()
            || resolution.created_at < appeal.created_at
            || resolution.created_at < evaluation.created_at
        {
            return Err(ApiError::conflict(
                "paper_finality_resolution_binding_mismatch",
                "effective evaluation may bind only its exact chronological terminal denial",
            ));
        }
        return Ok(Some(resolution.resolution_id));
    }

    let Some(parent_id) = evaluation.supersedes_evaluation_id else {
        return Ok(None);
    };
    let parent = evaluations
        .iter()
        .find(|candidate| {
            candidate.paper_project_id == evaluation.paper_project_id
                && candidate.evaluation_id == parent_id
        })
        .ok_or_else(|| {
            ApiError::conflict(
                "paper_finality_evaluation_lineage_missing",
                "effective replacement evaluation lost its parent binding",
            )
        })?;
    let appeal = exact_appeal(parent_id)?.ok_or_else(|| {
        ApiError::conflict(
            "paper_finality_appeal_binding_missing",
            "effective replacement evaluation lost its activating Appeal",
        )
    })?;
    let resolution = exact_resolution(appeal.appeal_id)?.ok_or_else(|| {
        ApiError::conflict(
            "paper_chain_finality_open_appeal",
            "effective replacement evaluation has no activating Appeal resolution",
        )
    })?;
    if appeal.release_candidate_hash != parent.release_candidate_hash
        || resolution.evaluation_id != parent_id
        || resolution.release_candidate_hash != parent.release_candidate_hash
        || resolution.outcome != AppealOutcome::Upheld
        || resolution.superseding_evaluation_id != Some(evaluation.evaluation_id)
        || resolution.created_at < appeal.created_at
        || resolution.created_at < evaluation.created_at
    {
        return Err(ApiError::conflict(
            "paper_finality_resolution_binding_mismatch",
            "effective replacement evaluation lost its exact chronological upheld activation",
        ));
    }
    Ok(Some(resolution.resolution_id))
}

#[allow(clippy::too_many_arguments)]
fn claimable_review_vacancies(
    paper_id: Uuid,
    submission_id: Uuid,
    player_id: Uuid,
    evaluations: &[PaperEvaluation],
    reproductions: &[PaperReproduction],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
    assignments: &[ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<Vec<ReviewAssignmentVacancy>, ApiError> {
    let latest = latest_review_evaluation_for_submission(paper_id, submission_id, evaluations);
    let mut vacancies = Vec::new();
    let panel_round = match latest {
        None => Some(1),
        Some(evaluation)
            if evaluation_has_open_appeal(evaluation.evaluation_id, appeals, resolutions) =>
        {
            Some(
                evaluation
                    .version
                    .checked_add(1)
                    .ok_or_else(|| ApiError::internal("review round overflow"))?,
            )
        }
        Some(_) => None,
    };
    if let Some(review_round) = panel_round {
        let was_prior_panel_member = evaluations.iter().any(|evaluation| {
            evaluation.paper_project_id == paper_id
                && evaluation.submission_id == submission_id
                && evaluation.version < review_round
                && (evaluation.evaluator_player_id == player_id
                    || evaluation
                        .reviewer_attestations
                        .iter()
                        .any(|review| review.reviewer_player_id == player_id))
        });
        if !was_prior_panel_member {
            vacancies.extend(
                [
                    ReviewAssignmentSlot::Evaluator,
                    ReviewAssignmentSlot::Reviewer1,
                    ReviewAssignmentSlot::Reviewer2,
                ]
                .into_iter()
                .map(|slot| ReviewAssignmentVacancy { review_round, slot }),
            );
        }
    }
    if let Some(evaluation) = latest.filter(|evaluation| {
        activated_evaluation_at(
            paper_id,
            evaluation.evaluation_id,
            evaluations,
            appeals,
            resolutions,
        )
        .is_some()
    }) {
        let reports: Vec<_> = reproductions
            .iter()
            .filter(|report| report.evaluation_id == evaluation.evaluation_id)
            .collect();
        let is_panel_member = evaluation.evaluator_player_id == player_id
            || evaluation
                .reviewer_attestations
                .iter()
                .any(|review| review.reviewer_player_id == player_id);
        if !is_panel_member
            && (reports.is_empty()
                || reports
                    .iter()
                    .all(|report| report.reproducer_player_id == player_id))
        {
            vacancies.push(ReviewAssignmentVacancy {
                review_round: evaluation.version,
                slot: ReviewAssignmentSlot::Reproducer,
            });
        }
    }
    vacancies.retain(|vacancy| {
        !assignments.iter().any(|assignment| {
            assignment.paper_project_id == paper_id
                && assignment.submission_id == submission_id
                && assignment.review_round == vacancy.review_round
                && assignment.slot == vacancy.slot
                && review_assignment_active(assignment, now)
        })
    });
    vacancies.sort_by_key(|vacancy| (vacancy.review_round, vacancy.slot.rank()));
    Ok(vacancies)
}

#[allow(clippy::too_many_arguments)]
fn make_review_queue_item(
    paper: &PaperProject,
    submission: &JointPaperSubmission,
    player_id: Uuid,
    evaluations: &[PaperEvaluation],
    reproductions: &[PaperReproduction],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
    assignments: &[ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<Option<ReviewQueueItem>, ApiError> {
    if paper.phase != PaperPhase::SubmissionReady
        || submission.status != JointSubmissionStatus::SubmissionReady
        || submission.paper_project_id != paper.paper_project_id
    {
        return Ok(None);
    }
    let mut my_assignments: Vec<_> = assignments
        .iter()
        .filter(|assignment| {
            assignment.paper_project_id == paper.paper_project_id
                && assignment.submission_id == submission.submission_id
                && assignment.player_id == player_id
                && review_assignment_active(assignment, now)
        })
        .cloned()
        .collect();
    my_assignments.sort_by_key(|assignment| (assignment.review_round, assignment.slot.rank()));
    let open_slots = claimable_review_vacancies(
        paper.paper_project_id,
        submission.submission_id,
        player_id,
        evaluations,
        reproductions,
        appeals,
        resolutions,
        assignments,
        now,
    )?;
    if open_slots.is_empty() && my_assignments.is_empty() {
        return Ok(None);
    }
    let author_count = u32::try_from(submission.paper_bundle.release_candidate.authors.len())
        .map_err(|_| ApiError::internal("PaperBundle author count overflow"))?;
    Ok(Some(ReviewQueueItem {
        paper_project_id: paper.paper_project_id,
        submission_id: submission.submission_id,
        challenge_id: paper.challenge_id,
        title: submission.paper_bundle.release_candidate.title.clone(),
        abstract_text: submission
            .paper_bundle
            .release_candidate
            .abstract_text
            .clone(),
        target_format: submission
            .paper_bundle
            .release_candidate
            .target_format
            .clone(),
        release_candidate_hash: submission.release_candidate_hash.clone(),
        paper_bundle_hash: submission.paper_bundle_hash.clone(),
        author_count,
        submitted_at: submission.created_at,
        my_assignments,
        open_slots,
    }))
}

fn is_author(context: &ReviewPaperContext, player_id: Uuid) -> bool {
    context
        .team
        .members
        .iter()
        .any(|member| member.player_id == player_id)
}

fn has_review_assignment<'a>(
    paper_id: Uuid,
    player_id: Uuid,
    mut assignments: impl Iterator<Item = &'a ReviewAssignment>,
    now: DateTime<Utc>,
) -> bool {
    assignments.any(|assignment| {
        assignment.paper_project_id == paper_id
            && assignment.player_id == player_id
            && review_assignment_active(assignment, now)
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_review_assignment_claim(
    context: &ReviewPaperContext,
    submission: &JointPaperSubmission,
    player_id: Uuid,
    requested: ReviewAssignmentVacancy,
    evaluations: &[PaperEvaluation],
    reproductions: &[PaperReproduction],
    appeals: &[PaperAppeal],
    resolutions: &[PaperAppealResolution],
    assignments: &[ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if context.paper.phase != PaperPhase::SubmissionReady
        || submission.status != JointSubmissionStatus::SubmissionReady
        || submission.paper_project_id != context.paper.paper_project_id
    {
        return Err(ApiError::conflict(
            "paper_not_submission_ready",
            "review assignments require a frozen submission-ready PaperBundle",
        ));
    }
    if is_author(context, player_id) {
        return Err(ApiError::forbidden(
            "review_assignment_author_forbidden",
            "authors cannot claim evaluator, reviewer, or reproducer assignments",
        ));
    }
    if assignments.iter().any(|assignment| {
        assignment.paper_project_id == context.paper.paper_project_id
            && assignment.submission_id == submission.submission_id
            && assignment.review_round == requested.review_round
            && assignment.player_id == player_id
            && review_assignment_active(assignment, now)
    }) {
        return Err(ApiError::conflict(
            "review_actor_already_assigned",
            "one player may hold only one active review role per Paper round",
        ));
    }
    let vacancies = claimable_review_vacancies(
        context.paper.paper_project_id,
        submission.submission_id,
        player_id,
        evaluations,
        reproductions,
        appeals,
        resolutions,
        assignments,
        now,
    )?;
    if !vacancies.contains(&requested) {
        return Err(ApiError::conflict(
            "review_assignment_not_claimable",
            "the requested Review Raid round and slot are not currently claimable",
        ));
    }
    if requested.slot != ReviewAssignmentSlot::Reproducer && requested.review_round > 1 {
        let prior_panel: HashSet<_> = evaluations
            .iter()
            .filter(|evaluation| {
                evaluation.paper_project_id == context.paper.paper_project_id
                    && evaluation.submission_id == submission.submission_id
                    && evaluation.version < requested.review_round
            })
            .flat_map(|evaluation| {
                std::iter::once(evaluation.evaluator_player_id).chain(
                    evaluation
                        .reviewer_attestations
                        .iter()
                        .map(|review| review.reviewer_player_id),
                )
            })
            .collect();
        if prior_panel.contains(&player_id) {
            return Err(ApiError::forbidden(
                "review_assignment_repanel_not_independent",
                "Appeal repanel assignments must be disjoint from prior evaluation panels",
            ));
        }
    }
    if requested.slot == ReviewAssignmentSlot::Reproducer {
        let evaluation = evaluations
            .iter()
            .find(|evaluation| {
                evaluation.paper_project_id == context.paper.paper_project_id
                    && evaluation.submission_id == submission.submission_id
                    && evaluation.version == requested.review_round
            })
            .ok_or_else(|| {
                ApiError::conflict(
                    "review_assignment_evaluation_missing",
                    "reproducer assignment requires an evaluation in the requested round",
                )
            })?;
        if evaluation.evaluator_player_id == player_id
            || evaluation
                .reviewer_attestations
                .iter()
                .any(|review| review.reviewer_player_id == player_id)
        {
            return Err(ApiError::forbidden(
                "review_assignment_reproducer_not_independent",
                "reproducer must be distinct from the assigned evaluation panel",
            ));
        }
    }
    Ok(())
}

fn enforce_evaluation_assignments<'a>(
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    request: &CreatePaperEvaluationRequest,
    assignments: impl Iterator<Item = &'a ReviewAssignment>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let assignments: Vec<_> = assignments
        .filter(|assignment| {
            assignment.paper_project_id == paper_id
                && assignment.submission_id == submission_id
                && assignment.review_round == review_round
                && review_assignment_active(assignment, now)
        })
        .collect();
    let assigned = |slot| {
        assignments
            .iter()
            .find(|assignment| assignment.slot == slot)
            .map(|assignment| assignment.player_id)
    };
    let reviewer_ids: HashSet<_> = request
        .reviewer_attestations
        .iter()
        .map(|review| review.reviewer_player_id)
        .collect();
    let expected_reviewers = [
        assigned(ReviewAssignmentSlot::Reviewer1),
        assigned(ReviewAssignmentSlot::Reviewer2),
    ];
    if assigned(ReviewAssignmentSlot::Evaluator) != Some(request.evaluator_player_id)
        || expected_reviewers.iter().any(Option::is_none)
        || expected_reviewers
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>()
            != reviewer_ids
    {
        return Err(ApiError::forbidden(
            "review_assignment_panel_mismatch",
            "evaluation actor and reviewers must exactly match the claimed Review Raid slots",
        ));
    }
    Ok(())
}

fn claimed_evaluation_panel_assignment_ids(
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    request: &CreatePaperEvaluationRequest,
    assignments: &[ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<Vec<Uuid>, ApiError> {
    let reviewer_ids = request
        .reviewer_attestations
        .iter()
        .map(|review| review.reviewer_player_id)
        .collect::<HashSet<_>>();
    let expected = [
        (ReviewAssignmentSlot::Evaluator, request.evaluator_player_id),
        (
            ReviewAssignmentSlot::Reviewer1,
            assignments
                .iter()
                .find(|assignment| {
                    assignment.paper_project_id == paper_id
                        && assignment.submission_id == submission_id
                        && assignment.review_round == review_round
                        && assignment.slot == ReviewAssignmentSlot::Reviewer1
                        && reviewer_ids.contains(&assignment.player_id)
                        && assignment.status == ReviewAssignmentStatus::Claimed
                        && assignment.pinned_evaluation_id.is_none()
                        && assignment.expires_at > now
                })
                .map(|assignment| assignment.player_id)
                .ok_or_else(|| {
                    ApiError::conflict(
                        "review_assignment_panel_changed",
                        "the frozen reviewer_1 assignment is no longer claimable",
                    )
                })?,
        ),
        (
            ReviewAssignmentSlot::Reviewer2,
            assignments
                .iter()
                .find(|assignment| {
                    assignment.paper_project_id == paper_id
                        && assignment.submission_id == submission_id
                        && assignment.review_round == review_round
                        && assignment.slot == ReviewAssignmentSlot::Reviewer2
                        && reviewer_ids.contains(&assignment.player_id)
                        && assignment.status == ReviewAssignmentStatus::Claimed
                        && assignment.pinned_evaluation_id.is_none()
                        && assignment.expires_at > now
                })
                .map(|assignment| assignment.player_id)
                .ok_or_else(|| {
                    ApiError::conflict(
                        "review_assignment_panel_changed",
                        "the frozen reviewer_2 assignment is no longer claimable",
                    )
                })?,
        ),
    ];
    let mut ids = Vec::with_capacity(expected.len());
    for (slot, player_id) in expected {
        let mut matching = assignments.iter().filter(|assignment| {
            assignment.paper_project_id == paper_id
                && assignment.submission_id == submission_id
                && assignment.review_round == review_round
                && assignment.slot == slot
                && assignment.player_id == player_id
                && assignment.status == ReviewAssignmentStatus::Claimed
                && assignment.pinned_evaluation_id.is_none()
                && assignment.expires_at > now
        });
        let assignment = matching.next().ok_or_else(|| {
            ApiError::conflict(
                "review_assignment_panel_changed",
                "the frozen evaluation panel is no longer claimable",
            )
        })?;
        if matching.next().is_some() {
            return Err(ApiError::internal(
                "multiple claimed Review assignments occupy one frozen panel slot",
            ));
        }
        ids.push(assignment.assignment_id);
    }
    if ids.len() != 3
        || ids.iter().copied().collect::<HashSet<_>>().len() != 3
        || reviewer_ids.len() != 2
    {
        return Err(ApiError::conflict(
            "review_assignment_panel_changed",
            "the frozen evaluation panel must contain exactly three distinct assignments",
        ));
    }
    Ok(ids)
}

fn pinned_evaluation_panel_assignment_ids(
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    request: &CreatePaperEvaluationRequest,
    assignments: &[ReviewAssignment],
) -> Result<Vec<Uuid>, ApiError> {
    let evaluator = assignments.iter().find(|assignment| {
        assignment.paper_project_id == paper_id
            && assignment.submission_id == submission_id
            && assignment.review_round == review_round
            && assignment.slot == ReviewAssignmentSlot::Evaluator
            && assignment.player_id == request.evaluator_player_id
            && assignment.status == ReviewAssignmentStatus::Pinned
            && assignment.pinned_evaluation_id == Some(request.evaluation_id)
    });
    let reviewer_ids = request
        .reviewer_attestations
        .iter()
        .map(|review| review.reviewer_player_id)
        .collect::<HashSet<_>>();
    let reviewers = [
        ReviewAssignmentSlot::Reviewer1,
        ReviewAssignmentSlot::Reviewer2,
    ]
    .into_iter()
    .map(|slot| {
        assignments.iter().find(|assignment| {
            assignment.paper_project_id == paper_id
                && assignment.submission_id == submission_id
                && assignment.review_round == review_round
                && assignment.slot == slot
                && reviewer_ids.contains(&assignment.player_id)
                && assignment.status == ReviewAssignmentStatus::Pinned
                && assignment.pinned_evaluation_id == Some(request.evaluation_id)
        })
    })
    .collect::<Vec<_>>();
    let Some(evaluator) = evaluator else {
        return Err(ApiError::conflict(
            "evaluation_draft_assignment_not_pinned",
            "evaluation draft finalization requires its evaluator assignment to remain pinned",
        ));
    };
    if reviewers.iter().any(|assignment| assignment.is_none()) {
        return Err(ApiError::conflict(
            "evaluation_draft_assignment_not_pinned",
            "evaluation draft finalization requires both attesting reviewer assignments to remain pinned",
        ));
    }
    let mut ids = vec![evaluator.assignment_id];
    ids.extend(
        reviewers
            .into_iter()
            .flatten()
            .map(|assignment| assignment.assignment_id),
    );
    if ids.len() != 3 || ids.iter().copied().collect::<HashSet<_>>().len() != 3 {
        return Err(ApiError::internal(
            "evaluation draft pinned panel assignment identities are not distinct",
        ));
    }
    Ok(ids)
}

fn enforce_reproducer_assignment<'a>(
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    reproducer_player_id: Uuid,
    assignments: impl Iterator<Item = &'a ReviewAssignment>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if !assignments.into_iter().any(|assignment| {
        assignment.paper_project_id == paper_id
            && assignment.submission_id == submission_id
            && assignment.review_round == review_round
            && assignment.slot == ReviewAssignmentSlot::Reproducer
            && assignment.player_id == reproducer_player_id
            && review_assignment_active(assignment, now)
    }) {
        return Err(ApiError::forbidden(
            "review_assignment_reproducer_mismatch",
            "reproduction actor must match the claimed Review Raid reproducer slot",
        ));
    }
    Ok(())
}

async fn load_review_assignments_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<ReviewAssignment>, ApiError> {
    let rows = sqlx::query(
        "select assignment_id,paper_project_id,submission_id,player_id,review_round,slot,pinned_evaluation_id,status,
                version,record_json,created_at,expires_at,updated_at
         from hepta_paper_review_assignments
         where paper_project_id=$1 order by slot, assignment_id for share",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    rows.iter().map(decode_review_assignment_row).collect()
}

fn decode_review_assignment_row(row: &PgRow) -> Result<ReviewAssignment, ApiError> {
    let assignment: ReviewAssignment = decode_record(row.get("record_json"), "review assignment")?;
    let version: i64 = row.get("version");
    let created_at: DateTime<Utc> = row.get("created_at");
    let expires_at: DateTime<Utc> = row.get("expires_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");
    if assignment.schema != REVIEW_ASSIGNMENT_SCHEMA_V1
        || assignment.assignment_id != row.get::<Uuid, _>("assignment_id")
        || assignment.paper_project_id != row.get::<Uuid, _>("paper_project_id")
        || assignment.submission_id != row.get::<Uuid, _>("submission_id")
        || assignment.player_id != row.get::<Uuid, _>("player_id")
        || i64::try_from(assignment.review_round).ok() != Some(row.get::<i64, _>("review_round"))
        || assignment.slot.as_str() != row.get::<String, _>("slot")
        || assignment.pinned_evaluation_id != row.get::<Option<Uuid>, _>("pinned_evaluation_id")
        || assignment.status.as_str() != row.get::<String, _>("status")
        || i64::try_from(assignment.version).ok() != Some(version)
        || assignment.claimed_at.timestamp_micros() != created_at.timestamp_micros()
        || assignment.expires_at.timestamp_micros() != expires_at.timestamp_micros()
        || assignment.updated_at.timestamp_micros() != updated_at.timestamp_micros()
    {
        return Err(ApiError::internal(
            "review assignment record does not match its durable index columns",
        ));
    }
    Ok(assignment)
}

fn transition_review_assignment(
    assignment: &mut ReviewAssignment,
    next_status: ReviewAssignmentStatus,
    pin_evaluation_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let permitted = matches!(
        (assignment.status, next_status),
        (
            ReviewAssignmentStatus::Claimed,
            ReviewAssignmentStatus::Pinned | ReviewAssignmentStatus::Expired
        ) | (
            ReviewAssignmentStatus::Pinned,
            ReviewAssignmentStatus::Consumed | ReviewAssignmentStatus::Expired
        )
    );
    let pin_valid = if next_status == ReviewAssignmentStatus::Pinned {
        assignment.pinned_evaluation_id.is_none() && pin_evaluation_id.is_some()
    } else {
        pin_evaluation_id.is_none()
    };
    if !permitted
        || !pin_valid
        || (next_status == ReviewAssignmentStatus::Pinned && assignment.expires_at <= now)
    {
        return Err(ApiError::conflict(
            "review_assignment_lifecycle_conflict",
            "review assignment is no longer eligible for the requested lifecycle transition",
        ));
    }
    if next_status == ReviewAssignmentStatus::Pinned {
        assignment.pinned_evaluation_id = pin_evaluation_id;
    }
    assignment.status = next_status;
    assignment.version = assignment
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("review assignment version overflow"))?;
    assignment.updated_at = now;
    Ok(())
}

fn transition_review_assignment_memory(
    review: &mut ReviewMemory,
    assignment_id: Uuid,
    next_status: ReviewAssignmentStatus,
    pin_evaluation_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let assignment = review
        .assignments
        .get_mut(&assignment_id)
        .ok_or_else(|| ApiError::internal("review assignment disappeared during transition"))?;
    transition_review_assignment(assignment, next_status, pin_evaluation_id, now)
}

async fn transition_review_assignment_postgres(
    tx: &mut Transaction<'_, Postgres>,
    assignment_id: Uuid,
    next_status: ReviewAssignmentStatus,
    pin_evaluation_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let row = sqlx::query(
        "select assignment_id,paper_project_id,submission_id,player_id,review_round,slot,pinned_evaluation_id,status,
                version,record_json,created_at,expires_at,updated_at
         from hepta_paper_review_assignments where assignment_id=$1 for update",
    )
    .bind(assignment_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::internal("review assignment disappeared during transition"))?;
    let mut assignment = decode_review_assignment_row(&row)?;
    let expected_version = assignment.version;
    let expected_status = assignment.status;
    transition_review_assignment(&mut assignment, next_status, pin_evaluation_id, now)?;
    let record_json = serde_json::to_value(&assignment)
        .map_err(|error| ApiError::internal(format!("encode review assignment: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_review_assignments
         set status=$1,pinned_evaluation_id=$2,version=$3,record_json=$4::jsonb,updated_at=$5
         where assignment_id=$6 and version=$7 and status=$8",
    )
    .bind(assignment.status.as_str())
    .bind(assignment.pinned_evaluation_id)
    .bind(i64::try_from(assignment.version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(record_json)
    .bind(now)
    .bind(assignment_id)
    .bind(i64::try_from(expected_version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(expected_status.as_str())
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "review_assignment_lifecycle_conflict",
            "review assignment changed during its lifecycle transition",
        ));
    }
    Ok(())
}

fn expire_review_assignments_memory(
    review: &mut ReviewMemory,
    paper_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    for assignment in review.assignments.values_mut().filter(|assignment| {
        assignment.paper_project_id == paper_id
            && assignment.status == ReviewAssignmentStatus::Claimed
            && assignment.expires_at <= now
    }) {
        transition_review_assignment(assignment, ReviewAssignmentStatus::Expired, None, now)?;
    }
    Ok(())
}

async fn expire_review_assignments_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<ReviewAssignment>, ApiError> {
    let mut assignments = load_review_assignments_postgres(tx, paper_id).await?;
    for assignment in assignments.iter_mut().filter(|assignment| {
        assignment.status == ReviewAssignmentStatus::Claimed && assignment.expires_at <= now
    }) {
        let expected_version = assignment.version;
        transition_review_assignment(assignment, ReviewAssignmentStatus::Expired, None, now)?;
        let record_json = serde_json::to_value(&assignment).map_err(|error| {
            ApiError::internal(format!("encode expired review assignment: {error}"))
        })?;
        let updated = sqlx::query(
            "update hepta_paper_review_assignments
             set status='expired',version=$1,record_json=$2::jsonb,updated_at=$3
             where assignment_id=$4 and version=$5 and status='claimed'",
        )
        .bind(
            i64::try_from(assignment.version)
                .map_err(|_| ApiError::internal("version overflow"))?,
        )
        .bind(record_json)
        .bind(now)
        .bind(assignment.assignment_id)
        .bind(i64::try_from(expected_version).map_err(|_| ApiError::internal("version overflow"))?)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "review_assignment_expiry_conflict",
                "review assignment changed while expiring its lease",
            ));
        }
    }
    Ok(assignments)
}

fn expire_stale_evaluation_drafts_memory(
    memory: &mut PaperRaidMemory,
    paper_id: Uuid,
    now: DateTime<Utc>,
    operation: &str,
    idempotency_key: &str,
) -> Result<(), ApiError> {
    let mut stale_ids = memory
        .review
        .evaluation_drafts
        .values()
        .filter(|draft| {
            draft.paper_project_id == paper_id
                && draft.status == EvaluationDraftStatus::Open
                && draft.expires_at <= now
        })
        .map(|draft| draft.evaluation_id)
        .collect::<Vec<_>>();
    stale_ids.sort_unstable();
    for evaluation_id in stale_ids {
        let draft = memory
            .review
            .evaluation_drafts
            .get(&evaluation_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("stale evaluation draft disappeared"))?;
        let attestations = memory
            .review
            .draft_attestations
            .values()
            .filter(|record| record.evaluation_id == evaluation_id)
            .cloned()
            .collect::<Vec<_>>();
        let assignments = memory
            .review
            .assignments
            .values()
            .filter(|assignment| assignment.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        let released_assignment_ids =
            expirable_pinned_assignment_ids(&draft, &attestations, &assignments)?;
        let expired = memory
            .review
            .evaluation_drafts
            .get_mut(&evaluation_id)
            .ok_or_else(|| ApiError::internal("stale evaluation draft disappeared"))?;
        if !expire_evaluation_draft(expired, now)? {
            continue;
        }
        let expired = expired.clone();
        for assignment_id in &released_assignment_ids {
            transition_review_assignment_memory(
                &mut memory.review,
                *assignment_id,
                ReviewAssignmentStatus::Expired,
                None,
                expired.expires_at,
            )?;
        }
        push_room_event_memory(
            memory,
            operation,
            &format!("{idempotency_key}:expired-draft:{evaluation_id}"),
            "hepta.paper_raid.evaluation_draft.expired.v1",
            paper_id,
            paper_id,
            expired.version,
            json!({
                "evaluation_id": evaluation_id,
                "review_round": expired.review_round,
                "draft_hash": expired.draft_hash,
                "lease_expires_at": expired.expires_at,
                "expired_at": expired.expired_at,
                "released_panel_assignment_ids": released_assignment_ids,
            }),
        );
    }
    Ok(())
}

async fn expire_stale_evaluation_drafts_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    now: DateTime<Utc>,
    operation: &str,
    idempotency_key: &str,
) -> Result<(), ApiError> {
    let rows = sqlx::query(
        "select record_json from hepta_paper_evaluation_drafts
         where paper_project_id=$1 and status='open' and expires_at <= $2
         order by evaluation_id for update",
    )
    .bind(paper_id)
    .bind(now)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    for row in rows {
        let mut draft = decode_evaluation_draft_row(&row)?;
        let attestations = load_draft_attestations_postgres(tx, draft.evaluation_id).await?;
        let assignments = load_review_assignments_postgres(tx, paper_id).await?;
        let released_assignment_ids =
            expirable_pinned_assignment_ids(&draft, &attestations, &assignments)?;
        let expected_version = draft.version;
        if !expire_evaluation_draft(&mut draft, now)? {
            continue;
        }
        let record_json = serde_json::to_value(&draft)
            .map_err(|error| ApiError::internal(format!("encode expired draft: {error}")))?;
        let updated = sqlx::query(
            "update hepta_paper_evaluation_drafts
             set status='expired',version=$1,record_json=$2::jsonb,updated_at=$3,expired_at=$3
             where evaluation_id=$4 and paper_project_id=$5 and status='open' and version=$6",
        )
        .bind(i64::try_from(draft.version).map_err(|_| ApiError::internal("version overflow"))?)
        .bind(record_json)
        .bind(draft.expires_at)
        .bind(draft.evaluation_id)
        .bind(paper_id)
        .bind(i64::try_from(expected_version).map_err(|_| ApiError::internal("version overflow"))?)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "evaluation_draft_expiry_conflict",
                "evaluation draft changed concurrently while its lease was expiring",
            ));
        }
        for assignment_id in &released_assignment_ids {
            transition_review_assignment_postgres(
                tx,
                *assignment_id,
                ReviewAssignmentStatus::Expired,
                None,
                draft.expires_at,
            )
            .await?;
        }
        insert_room_event_postgres(
            tx,
            operation,
            &format!("{idempotency_key}:expired-draft:{}", draft.evaluation_id),
            "hepta.paper_raid.evaluation_draft.expired.v1",
            paper_id,
            paper_id,
            draft.version,
            json!({
                "evaluation_id": draft.evaluation_id,
                "review_round": draft.review_round,
                "draft_hash": draft.draft_hash,
                "lease_expires_at": draft.expires_at,
                "expired_at": draft.expired_at,
                "released_panel_assignment_ids": released_assignment_ids,
            }),
        )
        .await?;
    }
    Ok(())
}

async fn get_review_queue(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ReviewQueueItem>>, ApiError> {
    const PATH: &str = "/v2/hepta/review-queue";
    let assertion = require_member_read_assertion(&headers, &state, "get_review_queue_v1", PATH)?;
    let now = Utc::now();
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        active_registered_player_memory(&memory, &assertion)?;
        let mut items = Vec::new();
        for submission in memory.submissions.values() {
            let Some(paper) = memory.papers.get(&submission.paper_project_id) else {
                continue;
            };
            let Some(team) = memory.teams.get(&paper.team_id) else {
                continue;
            };
            if team
                .members
                .iter()
                .any(|member| member.player_id == assertion.player_id)
            {
                continue;
            }
            let evaluations: Vec<_> = memory
                .review
                .evaluations
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect();
            let reproductions: Vec<_> = memory
                .review
                .reproductions
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect();
            let appeals: Vec<_> = memory
                .review
                .appeals
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect();
            let resolutions: Vec<_> = memory
                .review
                .resolutions
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect();
            let mut assignments: Vec<_> = memory
                .review
                .assignments
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect();
            let mut drafts = memory
                .review
                .evaluation_drafts
                .values()
                .filter(|record| record.paper_project_id == paper.paper_project_id)
                .cloned()
                .collect::<Vec<_>>();
            project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)?;
            if let Some(item) = make_review_queue_item(
                paper,
                submission,
                assertion.player_id,
                &evaluations,
                &reproductions,
                &appeals,
                &resolutions,
                &assignments,
                now,
            )? {
                items.push(item);
            }
        }
        items.sort_by_key(|item| (item.submitted_at, item.paper_project_id));
        return Ok(Json(items));
    }

    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    active_registered_player_postgres(&mut tx, &assertion).await?;
    let assignment_rows = sqlx::query(
        "select assignment_id,paper_project_id,submission_id,player_id,review_round,slot,pinned_evaluation_id,status,
                version,record_json,created_at,expires_at,updated_at
         from hepta_paper_review_assignments order by paper_project_id, slot",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let mut assignments: Vec<ReviewAssignment> = assignment_rows
        .iter()
        .map(decode_review_assignment_row)
        .collect::<Result<_, _>>()?;
    let draft_rows = sqlx::query(
        "select record_json from hepta_paper_evaluation_drafts order by paper_project_id, evaluation_id",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let mut drafts = draft_rows
        .iter()
        .map(decode_evaluation_draft_row)
        .collect::<Result<Vec<_>, _>>()?;
    project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)?;
    let rows = sqlx::query(
        "select p.record_json as paper_record, t.record_json as team_record,
                s.record_json as submission_record
         from hepta_paper_projects p
         join hepta_research_teams t on t.team_id=p.team_id
         join hepta_joint_paper_submissions s on s.paper_project_id=p.paper_project_id
         where p.phase='submission_ready' and s.status='submission_ready'
         order by s.created_at, p.paper_project_id",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let mut items = Vec::new();
    for row in rows {
        let paper: PaperProject = decode_record(row.get("paper_record"), "paper project")?;
        let team: ResearchTeam = decode_record(row.get("team_record"), "research team")?;
        if team
            .members
            .iter()
            .any(|member| member.player_id == assertion.player_id)
        {
            continue;
        }
        let submission: JointPaperSubmission =
            decode_record(row.get("submission_record"), "joint paper submission")?;
        let evaluations = load_review_records(
            &mut tx,
            "hepta_paper_evaluations",
            paper.paper_project_id,
            "paper evaluation",
        )
        .await?;
        let reproductions = load_review_records(
            &mut tx,
            "hepta_paper_reproductions",
            paper.paper_project_id,
            "paper reproduction",
        )
        .await?;
        let appeals = load_review_records(
            &mut tx,
            "hepta_paper_appeals",
            paper.paper_project_id,
            "paper Appeal",
        )
        .await?;
        let resolutions = load_review_records(
            &mut tx,
            "hepta_paper_appeal_resolutions",
            paper.paper_project_id,
            "Appeal resolution",
        )
        .await?;
        if let Some(item) = make_review_queue_item(
            &paper,
            &submission,
            assertion.player_id,
            &evaluations,
            &reproductions,
            &appeals,
            &resolutions,
            &assignments,
            now,
        )? {
            items.push(item);
        }
    }
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(items))
}

async fn claim_review_assignment(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<ClaimReviewAssignmentRequest>,
) -> Result<(StatusCode, Json<ReviewAssignment>), ApiError> {
    const OPERATION: &str = "claim_paper_review_assignment_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    if request.review_round == 0 {
        return Err(ApiError::bad_request(
            "invalid_review_round",
            "review_round must be positive",
        ));
    }
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/review-assignments");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.player_id {
        return Err(ApiError::forbidden(
            "review_assignment_assertion_mismatch",
            "Consumer assertion must identify the player claiming the review assignment",
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
        active_registered_player_memory(&next, &assertion)?;
        expire_stale_evaluation_drafts_memory(
            &mut next,
            paper_id,
            now,
            OPERATION,
            &request.idempotency_key,
        )?;
        expire_review_assignments_memory(&mut next.review, paper_id, now)?;
        let context = review_context_memory(&next, paper_id)?;
        let submission = next
            .submissions
            .values()
            .find(|submission| {
                submission.paper_project_id == paper_id
                    && submission.status == JointSubmissionStatus::SubmissionReady
            })
            .cloned()
            .ok_or_else(|| {
                ApiError::conflict(
                    "paper_not_submission_ready",
                    "review assignments require a frozen submission-ready PaperBundle",
                )
            })?;
        let release_manifest = exact_frozen_review_manifest(
            next.collaboration.artifact_manifests.values(),
            &submission,
        )?;
        validate_review_ready_artifact_manifest(&release_manifest)?;
        if next.review.assignments.contains_key(&request.assignment_id) {
            return Err(ApiError::conflict(
                "review_assignment_exists",
                "assignment_id already exists",
            ));
        }
        let evaluations: Vec<_> = next.review.evaluations.values().cloned().collect();
        let reproductions: Vec<_> = next.review.reproductions.values().cloned().collect();
        let appeals: Vec<_> = next.review.appeals.values().cloned().collect();
        let resolutions: Vec<_> = next.review.resolutions.values().cloned().collect();
        let assignments: Vec<_> = next.review.assignments.values().cloned().collect();
        let requested = ReviewAssignmentVacancy {
            review_round: request.review_round,
            slot: request.slot,
        };
        validate_review_assignment_claim(
            &context,
            &submission,
            request.player_id,
            requested,
            &evaluations,
            &reproductions,
            &appeals,
            &resolutions,
            &assignments,
            now,
        )?;
        let expires_at = now
            .checked_add_signed(chrono::Duration::hours(REVIEW_ASSIGNMENT_TTL_HOURS))
            .ok_or_else(|| ApiError::internal("review assignment expiry overflow"))?;
        let assignment = ReviewAssignment {
            schema: REVIEW_ASSIGNMENT_SCHEMA_V1.to_string(),
            assignment_id: request.assignment_id,
            paper_project_id: paper_id,
            submission_id: submission.submission_id,
            player_id: request.player_id,
            review_round: request.review_round,
            slot: request.slot,
            pinned_evaluation_id: None,
            status: ReviewAssignmentStatus::Claimed,
            version: 1,
            claimed_at: now,
            expires_at,
            updated_at: now,
        };
        next.review
            .assignments
            .insert(assignment.assignment_id, assignment.clone());
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.review_assignment.claimed.v1",
            paper_id,
            paper_id,
            1,
            json!({"assignment_id":assignment.assignment_id,"review_round":assignment.review_round,"slot":assignment.slot,"expires_at":assignment.expires_at}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &assignment,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(assignment)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    sqlx::query(
        "select paper_project_id from hepta_paper_projects where paper_project_id=$1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    active_registered_player_postgres(&mut tx, &assertion).await?;
    expire_stale_evaluation_drafts_postgres(
        &mut tx,
        paper_id,
        now,
        OPERATION,
        &request.idempotency_key,
    )
    .await?;
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id=$1 and status='submission_ready' for share",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_not_submission_ready",
            "review assignments require a frozen submission-ready PaperBundle",
        )
    })?;
    let submission: JointPaperSubmission =
        decode_record(submission_row.get("record_json"), "joint paper submission")?;
    let manifest_rows = sqlx::query(
        "select record_json from hepta_artifact_manifests
         where paper_project_id=$1 and manifest_hash=$2 for share",
    )
    .bind(paper_id)
    .bind(
        &submission
            .paper_bundle
            .release_candidate
            .artifact_manifest_hash,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let release_manifests = manifest_rows
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "review-ready release manifest"))
        .collect::<Result<Vec<ArtifactManifest>, ApiError>>()?;
    let release_manifest = exact_frozen_review_manifest(release_manifests.iter(), &submission)?;
    validate_review_ready_artifact_manifest(&release_manifest)?;
    let assignments = expire_review_assignments_postgres(&mut tx, paper_id, now).await?;
    let evaluations = load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let reproductions = load_review_records(
        &mut tx,
        "hepta_paper_reproductions",
        paper_id,
        "paper reproduction",
    )
    .await?;
    let appeals =
        load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal").await?;
    let resolutions = load_review_records(
        &mut tx,
        "hepta_paper_appeal_resolutions",
        paper_id,
        "Appeal resolution",
    )
    .await?;
    validate_review_assignment_claim(
        &context,
        &submission,
        request.player_id,
        ReviewAssignmentVacancy {
            review_round: request.review_round,
            slot: request.slot,
        },
        &evaluations,
        &reproductions,
        &appeals,
        &resolutions,
        &assignments,
        now,
    )?;
    let expires_at = now
        .checked_add_signed(chrono::Duration::hours(REVIEW_ASSIGNMENT_TTL_HOURS))
        .ok_or_else(|| ApiError::internal("review assignment expiry overflow"))?;
    let assignment = ReviewAssignment {
        schema: REVIEW_ASSIGNMENT_SCHEMA_V1.to_string(),
        assignment_id: request.assignment_id,
        paper_project_id: paper_id,
        submission_id: submission.submission_id,
        player_id: request.player_id,
        review_round: request.review_round,
        slot: request.slot,
        pinned_evaluation_id: None,
        status: ReviewAssignmentStatus::Claimed,
        version: 1,
        claimed_at: now,
        expires_at,
        updated_at: now,
    };
    let assignment_json = serde_json::to_value(&assignment)
        .map_err(|error| ApiError::internal(format!("encode review assignment: {error}")))?;
    let inserted = sqlx::query(
        "insert into hepta_paper_review_assignments
         (assignment_id,paper_project_id,submission_id,player_id,review_round,slot,
          pinned_evaluation_id,status,version,record_json,created_at,expires_at,updated_at)
         values ($1,$2,$3,$4,$5,$6,null,'claimed',1,$7::jsonb,$8,$9,$8)",
    )
    .bind(assignment.assignment_id)
    .bind(assignment.paper_project_id)
    .bind(assignment.submission_id)
    .bind(assignment.player_id)
    .bind(
        i64::try_from(assignment.review_round)
            .map_err(|_| ApiError::internal("review round overflow"))?,
    )
    .bind(assignment.slot.as_str())
    .bind(assignment_json)
    .bind(assignment.claimed_at)
    .bind(assignment.expires_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "review_assignment_conflict",
                "assignment identity, player, or slot was claimed concurrently",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.review_assignment.claimed.v1",
        paper_id,
        paper_id,
        1,
        json!({"assignment_id":assignment.assignment_id,"review_round":assignment.review_round,"slot":assignment.slot,"expires_at":assignment.expires_at}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(assignment.assignment_id),
        StatusCode::CREATED,
        &assignment,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

type ChallengeReviewObjectHashes = (String, String);

async fn paper_challenge_review_object_hashes(
    state: &AppState,
    paper_id: Uuid,
) -> Result<Option<ChallengeReviewObjectHashes>, ApiError> {
    let challenge_id = if let Some(pool) = &state.pool {
        sqlx::query_scalar(
            "select challenge_id from hepta_paper_projects where paper_project_id=$1",
        )
        .bind(paper_id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
    } else {
        let memory = state.paper_raid.read().await;
        memory.papers.get(&paper_id).map(|paper| paper.challenge_id)
    };
    let Some(challenge_id) = challenge_id else {
        return Ok(None);
    };
    state
        .inspect(|league| {
            Ok(league.challenges.get(&challenge_id).map(|challenge| {
                (
                    challenge.evaluator_manifest_hash.clone(),
                    challenge.dataset_manifest_hash.clone(),
                )
            }))
        })
        .await
}

async fn get_paper_review_bundle(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PaperReviewBundleV1>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/review-bundle");
    let assertion =
        require_member_read_assertion(&headers, &state, "get_paper_review_bundle_v1", &path)?;
    let now = Utc::now();
    // Resolve this before acquiring the Paper memory guard or PostgreSQL
    // review transaction. `AppState::inspect` reads the durable league-state
    // row for PostgreSQL and the in-process authority for the memory backend.
    let challenge_object_hashes = paper_challenge_review_object_hashes(&state, paper_id).await?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        active_registered_player_memory(&memory, &assertion)?;
        let context = review_context_memory(&memory, paper_id)?;
        let submission = memory
            .submissions
            .values()
            .find(|submission| {
                submission.paper_project_id == paper_id
                    && submission.status == JointSubmissionStatus::SubmissionReady
            })
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "review_bundle_not_found",
                    "submission-ready review bundle does not exist",
                )
            })?;
        let mut assignments = memory
            .review
            .assignments
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut drafts = memory
            .review
            .evaluation_drafts
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)?;
        if is_author(&context, assertion.player_id)
            || !has_review_assignment(paper_id, assertion.player_id, assignments.iter(), now)
        {
            return Err(ApiError::forbidden(
                "review_bundle_access_denied",
                "review bundle is limited to active independent review assignments; it never grants Author Room access",
            ));
        }
        let artifact_manifest = exact_frozen_review_manifest(
            memory.collaboration.artifact_manifests.values(),
            &submission,
        )?;
        let (evaluator_version, dataset_version) = challenge_object_hashes.ok_or_else(|| {
            ApiError::conflict(
                "frozen_evaluator_unavailable",
                "the Paper challenge has no immutable evaluator manifest authority",
            )
        })?;
        return Ok(Json(make_paper_review_bundle(PaperReviewBundleInput {
            submission,
            player_id: assertion.player_id,
            assignments,
            artifact_manifest,
            evaluator_version: &evaluator_version,
            dataset_version: &dataset_version,
            drafts,
            draft_attestations: memory
                .review
                .draft_attestations
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
            now,
        })?));
    }

    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    active_registered_player_postgres(&mut tx, &assertion).await?;
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id=$1 and status='submission_ready' for share",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "review_bundle_not_found",
            "submission-ready review bundle does not exist",
        )
    })?;
    let submission: JointPaperSubmission =
        decode_record(submission_row.get("record_json"), "joint paper submission")?;
    let mut assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    let mut drafts = load_review_records(
        &mut tx,
        "hepta_paper_evaluation_drafts",
        paper_id,
        "evaluation draft",
    )
    .await?;
    project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)?;
    if is_author(&context, assertion.player_id)
        || !has_review_assignment(paper_id, assertion.player_id, assignments.iter(), now)
    {
        return Err(ApiError::forbidden(
            "review_bundle_access_denied",
            "review bundle is limited to active independent review assignments; it never grants Author Room access",
        ));
    }
    let manifest_row = sqlx::query(
        "select record_json from hepta_artifact_manifests
         where paper_project_id=$1 and manifest_hash=$2 for share",
    )
    .bind(paper_id)
    .bind(
        &submission
            .paper_bundle
            .release_candidate
            .artifact_manifest_hash,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "frozen_review_manifest_unavailable",
            "the frozen release candidate does not resolve to one exact ArtifactManifest",
        )
    })?;
    let artifact_manifest: ArtifactManifest = decode_record(
        manifest_row.get("record_json"),
        "frozen review artifact manifest",
    )?;
    let (evaluator_version, dataset_version) = challenge_object_hashes.ok_or_else(|| {
        ApiError::conflict(
            "frozen_evaluator_unavailable",
            "the Paper challenge has no immutable evaluator manifest authority",
        )
    })?;
    let draft_attestations = load_review_records(
        &mut tx,
        "hepta_paper_evaluation_draft_attestations",
        paper_id,
        "evaluation draft attestation",
    )
    .await?;
    let evaluations = load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let response = make_paper_review_bundle(PaperReviewBundleInput {
        submission,
        player_id: assertion.player_id,
        assignments,
        artifact_manifest,
        evaluator_version: &evaluator_version,
        dataset_version: &dataset_version,
        drafts,
        draft_attestations,
        evaluations,
        now,
    })?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(response))
}

fn exact_frozen_review_manifest<'a>(
    manifests: impl Iterator<Item = &'a ArtifactManifest>,
    submission: &JointPaperSubmission,
) -> Result<ArtifactManifest, ApiError> {
    let expected_hash = &submission
        .paper_bundle
        .release_candidate
        .artifact_manifest_hash;
    let matches = manifests
        .filter(|manifest| {
            manifest.paper_project_id == submission.paper_project_id
                && manifest.manifest_hash == *expected_hash
        })
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(ApiError::conflict(
            "frozen_review_manifest_unavailable",
            "the frozen release candidate must resolve to exactly one ArtifactManifest",
        ));
    }
    Ok(matches[0].clone())
}

fn paper_scoped_review_object_key(
    paper_id: Uuid,
    artifact_manifest_hash: &str,
    object: &NeutralArtifactObjectV1,
) -> Result<String, ApiError> {
    let digest = format!("sha256:{}", object.sha256);
    let object_scope = canonical_json_sha256(&json!({
        "schema": "hepta.paper_raid.review_object_scope.v1",
        "paper_project_id": paper_id,
        "artifact_manifest_hash": artifact_manifest_hash,
        "logical_path": &object.logical_path,
        "digest": digest,
    }))
    .map_err(|error| ApiError::internal(format!("hash review object scope: {error}")))?;
    Ok(format!(
        "review-object-{}",
        object_scope
            .strip_prefix("sha256:")
            .expect("canonical digest has sha256 prefix")
    ))
}

fn make_frozen_review_authority(
    submission: &JointPaperSubmission,
    assignment: &ReviewAssignment,
    manifest: &ArtifactManifest,
    evaluator_manifest_hash: &str,
    dataset_manifest_hash: &str,
    now: DateTime<Utc>,
) -> Result<FrozenReviewAuthorityV1, ApiError> {
    if assignment.paper_project_id != submission.paper_project_id
        || assignment.submission_id != submission.submission_id
        || !review_assignment_active(assignment, now)
        || manifest.paper_project_id != submission.paper_project_id
        || manifest.manifest_hash
            != submission
                .paper_bundle
                .release_candidate
                .artifact_manifest_hash
    {
        return Err(ApiError::forbidden(
            "frozen_review_assignment_mismatch",
            "the active assignment, submission, and immutable manifest do not share one exact scope",
        ));
    }
    let location_by_path = manifest
        .storage_locations
        .iter()
        .map(|location| (location.logical_path.as_str(), location))
        .collect::<HashMap<_, _>>();
    if location_by_path.len() != manifest.storage_locations.len()
        || manifest.objects.len() != manifest.storage_locations.len()
    {
        return Err(ApiError::conflict(
            "frozen_review_object_mapping_invalid",
            "the frozen manifest does not have a one-to-one object/CAS mapping",
        ));
    }
    let mut objects = Vec::with_capacity(manifest.objects.len());
    for object in &manifest.objects {
        let location = location_by_path
            .get(object.logical_path.as_str())
            .ok_or_else(|| {
                ApiError::conflict(
                    "frozen_review_object_mapping_invalid",
                    "one frozen object has no exact CAS mapping",
                )
            })?;
        let digest = format!("sha256:{}", object.sha256);
        if location.sha256 != object.sha256
            || location.uri != format!("cas://sha256/{}", object.sha256)
            || !matches!(
                location.acl,
                ArtifactAcl::Reviewers | ArtifactAcl::PublicAfterRelease
            )
        {
            return Err(ApiError::forbidden(
                "frozen_review_object_not_disclosed",
                "every frozen review object must have one reviewer-readable immutable CAS location",
            ));
        }
        if !matches!(
            object.role.as_str(),
            "frozen_evaluator" | "evaluator_support" | "candidate" | "dataset" | "input"
        ) {
            // The release ArtifactManifest also contains human-readable paper, bibliography,
            // evidence, and audit objects.  Those remain committed by artifact_manifest_hash but
            // are not executable challenge-manifest members and must not enter the Bridge bundle.
            continue;
        }
        objects.push(FrozenReviewObjectV1 {
            object_key: paper_scoped_review_object_key(
                submission.paper_project_id,
                &manifest.manifest_hash,
                object,
            )?,
            logical_path: object.logical_path.clone(),
            role: object.role.clone(),
            digest,
            size_bytes: object.size,
            media_type: object.media_type.clone(),
            download_path: REVIEW_OBJECT_DOWNLOAD_PATH_V1.to_string(),
        });
    }
    objects.sort_by(|left, right| {
        (&left.object_key, &left.logical_path).cmp(&(&right.object_key, &right.logical_path))
    });
    let trusted_evaluators = objects
        .iter()
        .filter(|object| object.role == "frozen_evaluator")
        .count();
    let trusted_datasets = objects
        .iter()
        .filter(|object| object.role == "dataset")
        .count();
    let candidates = objects
        .iter()
        .filter(|object| object.role == "candidate")
        .count();
    if trusted_evaluators == 0 || trusted_datasets == 0 || candidates != 1 {
        return Err(ApiError::conflict(
            "frozen_review_objects_unavailable",
            "review resolution requires evaluator, dataset, and exactly one candidate members in the release ArtifactManifest",
        ));
    }
    let kind = match assignment.slot {
        ReviewAssignmentSlot::Evaluator => "evaluate",
        ReviewAssignmentSlot::Reviewer1 | ReviewAssignmentSlot::Reviewer2 => "review",
        ReviewAssignmentSlot::Reproducer => "reproduce",
    };
    let seed_bytes = assignment.assignment_id.as_bytes();
    let seed = u64::from_be_bytes(
        seed_bytes[..8]
            .try_into()
            .expect("UUID prefix is eight bytes"),
    ) & 9_007_199_254_740_991;
    let mut authority = FrozenReviewAuthorityV1 {
        schema: FROZEN_REVIEW_AUTHORITY_V1.to_string(),
        authority_hash: String::new(),
        assignment_id: assignment.assignment_id,
        paper_project_id: assignment.paper_project_id,
        submission_id: assignment.submission_id,
        review_round: assignment.review_round,
        slot: assignment.slot.as_str().to_string(),
        assignment_version: assignment.version,
        expires_at: assignment
            .expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
        release_candidate_hash: submission.release_candidate_hash.clone(),
        paper_bundle_hash: submission.paper_bundle_hash.clone(),
        artifact_manifest_hash: manifest.manifest_hash.clone(),
        evaluator_manifest_hash: evaluator_manifest_hash.to_string(),
        dataset_manifest_hash: dataset_manifest_hash.to_string(),
        artifact_objects: objects,
        execution_policy: FrozenReviewExecutionPolicyV1 {
            schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
            kind: kind.to_string(),
            adapter: "python3-stdlib-v1".to_string(),
            timeout_ms: 30_000,
            seed,
        },
    };
    authority.authority_hash = frozen_review_authority_hash(&authority)
        .map_err(|error| ApiError::internal(format!("hash frozen review authority: {error}")))?;
    verify_frozen_review_authority(&authority)
        .map_err(|error| ApiError::internal(format!("verify frozen review authority: {error}")))?;
    Ok(authority)
}

struct PaperReviewBundleInput<'a> {
    submission: JointPaperSubmission,
    player_id: Uuid,
    assignments: Vec<ReviewAssignment>,
    artifact_manifest: ArtifactManifest,
    evaluator_version: &'a str,
    dataset_version: &'a str,
    drafts: Vec<PaperEvaluationDraft>,
    draft_attestations: Vec<EvaluationDraftAttestation>,
    evaluations: Vec<PaperEvaluation>,
    now: DateTime<Utc>,
}

fn make_paper_review_bundle(
    input: PaperReviewBundleInput<'_>,
) -> Result<PaperReviewBundleV1, ApiError> {
    let PaperReviewBundleInput {
        submission,
        player_id,
        mut assignments,
        artifact_manifest,
        evaluator_version,
        dataset_version,
        mut drafts,
        draft_attestations,
        evaluations,
        now,
    } = input;
    project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)?;
    let mut my_assignments = assignments
        .iter()
        .filter(|assignment| {
            assignment.submission_id == submission.submission_id
                && assignment.player_id == player_id
                && review_assignment_active(assignment, now)
        })
        .cloned()
        .collect::<Vec<_>>();
    my_assignments.sort_by_key(|assignment| (assignment.review_round, assignment.slot.rank()));
    let active_rounds = my_assignments
        .iter()
        .map(|assignment| assignment.review_round)
        .collect::<HashSet<_>>();
    let draft = drafts
        .into_iter()
        .filter(|draft| {
            draft.submission_id == submission.submission_id
                && active_rounds.contains(&draft.review_round)
        })
        .max_by_key(|draft| (draft.review_round, draft.created_at, draft.evaluation_id));
    let evaluation_quorum = draft
        .map(|draft| {
            let attestations = draft_attestations
                .iter()
                .filter(|record| record.evaluation_id == draft.evaluation_id)
                .cloned()
                .collect();
            evaluation_draft_quorum(draft, attestations, &assignments, now)
        })
        .transpose()?;
    let evaluation = evaluations
        .into_iter()
        .filter(|evaluation| {
            evaluation.submission_id == submission.submission_id
                && active_rounds.contains(&evaluation.version)
        })
        .max_by_key(|evaluation| {
            (
                evaluation.version,
                evaluation.created_at,
                evaluation.evaluation_id,
            )
        });
    if my_assignments.len() != 1 {
        return Err(ApiError::conflict(
            "review_assignment_scope_ambiguous",
            "one review bundle request must resolve to exactly one active assignment",
        ));
    }
    let frozen_review_authority = make_frozen_review_authority(
        &submission,
        &my_assignments[0],
        &artifact_manifest,
        evaluator_version,
        dataset_version,
        now,
    )?;
    Ok(PaperReviewBundleV1 {
        submission,
        my_assignments,
        frozen_review_authority,
        evaluation_quorum,
        evaluation,
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

fn normalize_uuid_set(field: &'static str, values: &[Uuid]) -> Result<Vec<Uuid>, ApiError> {
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

fn duplicate_contribution_reference_kind(entries: &[CreditContribution]) -> Option<&'static str> {
    let mut artifacts = HashSet::new();
    let mut reviews = HashSet::new();
    for entry in entries {
        if entry
            .accepted_artifact_manifest_ids
            .iter()
            .any(|identifier| !artifacts.insert(*identifier))
        {
            return Some("artifact manifest");
        }
        if entry
            .accepted_section_review_ids
            .iter()
            .any(|identifier| !reviews.insert(*identifier))
        {
            return Some("section review");
        }
    }
    None
}

pub(super) fn contribution_ledger_hash(
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

fn validate_canonical_contribution_entries(entries: &[CreditContribution]) -> Result<(), ApiError> {
    if !(3..=5).contains(&entries.len()) {
        return Err(ApiError::internal(
            "frozen contribution ledger must contain exactly 3-5 authors",
        ));
    }
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| entry.player_id);
    if sorted != entries
        || sorted
            .windows(2)
            .any(|pair| pair[0].player_id == pair[1].player_id)
    {
        return Err(ApiError::internal(
            "frozen contribution ledger author entries are not canonical and unique",
        ));
    }
    if duplicate_contribution_reference_kind(entries).is_some() {
        return Err(ApiError::internal(
            "frozen contribution ledger credits one reference to multiple authors",
        ));
    }
    for entry in entries {
        let roles = normalize_string_set("credit_roles", &entry.credit_roles).map_err(|_| {
            ApiError::internal("frozen contribution ledger credit roles are not canonical")
        })?;
        let artifacts = normalize_uuid_set(
            "accepted_artifact_manifest_ids",
            &entry.accepted_artifact_manifest_ids,
        )
        .map_err(|_| {
            ApiError::internal("frozen contribution ledger artifact references are not canonical")
        })?;
        let reviews = normalize_uuid_set(
            "accepted_section_review_ids",
            &entry.accepted_section_review_ids,
        )
        .map_err(|_| {
            ApiError::internal("frozen contribution ledger review references are not canonical")
        })?;
        if roles != entry.credit_roles
            || artifacts != entry.accepted_artifact_manifest_ids
            || reviews != entry.accepted_section_review_ids
            || entry.contribution_points
                != milestone_contribution_points(artifacts.len(), reviews.len())
        {
            return Err(ApiError::internal(
                "frozen contribution ledger entry disagrees with canonical milestone semantics",
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_loaded_contribution_ledger(
    ledger: ContributionLedger,
    relational_id: Uuid,
    relational_paper_id: Uuid,
    relational_release_candidate_hash: &str,
    relational_ledger_hash: &str,
    relational_entries: &serde_json::Value,
    relational_version: u64,
    relational_created_at: DateTime<Utc>,
) -> Result<ContributionLedger, ApiError> {
    let encoded_entries = serde_json::to_value(&ledger.entries).map_err(|error| {
        ApiError::internal(format!("encode frozen contribution entries: {error}"))
    })?;
    if ledger.schema != CONTRIBUTION_LEDGER_SCHEMA_V1
        || ledger.contribution_ledger_id.is_nil()
        || ledger.contribution_ledger_id != relational_id
        || ledger.paper_project_id != relational_paper_id
        || ledger.release_candidate_hash != relational_release_candidate_hash
        || ledger.ledger_hash != relational_ledger_hash
        || encoded_entries != *relational_entries
        || ledger.version != relational_version
        // PostgreSQL timestamptz has microsecond precision while the JSON
        // envelope may retain additional chrono nanoseconds.
        || ledger.created_at.timestamp_micros() != relational_created_at.timestamp_micros()
    {
        return Err(ApiError::internal(
            "contribution ledger relational columns disagree with record_json",
        ));
    }
    validate_canonical_contribution_entries(&ledger.entries)?;
    let canonical_hash = contribution_ledger_hash(
        ledger.contribution_ledger_id,
        ledger.paper_project_id,
        &ledger.entries,
    )?;
    if canonical_hash != relational_ledger_hash {
        return Err(ApiError::internal(
            "contribution ledger canonical hash disagrees with frozen relational authority",
        ));
    }
    Ok(ledger)
}

fn milestone_contribution_points(artifact_count: usize, review_count: usize) -> u64 {
    (if artifact_count == 0 {
        0
    } else {
        ACCEPTED_ARTIFACT_MILESTONE_XP
    }) + if review_count == 0 {
        0
    } else {
        ACCEPTED_REVIEW_MILESTONE_XP
    }
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
        )?;
        let reviews = normalize_uuid_set(
            "accepted_section_review_ids",
            &input.accepted_section_review_ids,
        )?;
        // Contribution points are milestone-capped. Splitting one logical
        // artifact or review into many records must never mint more XP.
        let points = milestone_contribution_points(artifacts.len(), reviews.len());
        entries.push(CreditContribution {
            player_id: input.player_id,
            credit_roles: roles,
            accepted_artifact_manifest_ids: artifacts,
            accepted_section_review_ids: reviews,
            contribution_points: points,
        });
    }
    entries.sort_by_key(|entry| entry.player_id);
    if let Some(kind) = duplicate_contribution_reference_kind(&entries) {
        return Err(ApiError::bad_request(
            "duplicate_contribution_reference",
            format!("one {kind} identifier may be credited to only one author"),
        ));
    }
    Ok(entries)
}

fn authoritative_contribution_entries_from_records(
    paper_id: Uuid,
    authors: &[crate::paper_raid_contracts::PaperReleaseAuthorV2],
    artifact_manifests: &[ArtifactManifest],
    proposals: &[AgentProposal],
    decisions: &[HumanDecision],
    section_reviews: &[SectionReview],
) -> Result<Vec<CreditContribution>, ApiError> {
    if !(3..=5).contains(&authors.len()) {
        return Err(ApiError::conflict(
            "release_author_roster_mismatch",
            "authoritative contribution derivation requires the frozen 3-5 author roster",
        ));
    }

    let mut entries = HashMap::new();
    for author in authors {
        let roles = normalize_string_set("release_credit_roles", &author.credit_roles)?;
        if entries
            .insert(
                author.player_id,
                CreditContribution {
                    player_id: author.player_id,
                    credit_roles: roles,
                    accepted_artifact_manifest_ids: Vec::new(),
                    accepted_section_review_ids: Vec::new(),
                    contribution_points: 0,
                },
            )
            .is_some()
        {
            return Err(ApiError::conflict(
                "release_author_roster_mismatch",
                "frozen release candidate contains a duplicate author",
            ));
        }
    }

    let mut manifest_ids = HashSet::new();
    for manifest in artifact_manifests {
        if manifest.paper_project_id != paper_id || !manifest_ids.insert(manifest.manifest_id) {
            return Err(ApiError::internal(
                "authoritative contribution manifest set is cross-paper or duplicated",
            ));
        }
    }

    let mut proposal_ids = HashSet::new();
    for proposal in proposals {
        if proposal.paper_project_id != paper_id || !proposal_ids.insert(proposal.proposal_id) {
            return Err(ApiError::internal(
                "authoritative contribution proposal set is cross-paper or duplicated",
            ));
        }
    }
    let mut decisions_by_proposal: HashMap<Uuid, Vec<&HumanDecision>> = HashMap::new();
    let mut decision_ids = HashSet::new();
    for decision in decisions {
        if decision.paper_project_id != paper_id || !decision_ids.insert(decision.decision_id) {
            return Err(ApiError::internal(
                "authoritative contribution decision set is cross-paper or duplicated",
            ));
        }
        if !proposal_ids.contains(&decision.proposal_id) {
            return Err(ApiError::internal(
                "authoritative contribution decision references an unknown proposal",
            ));
        }
        decisions_by_proposal
            .entry(decision.proposal_id)
            .or_default()
            .push(decision);
    }

    let mut credited_manifest_ids = HashSet::new();
    for proposal in proposals {
        if proposal.status != AgentProposalStatus::Accepted {
            continue;
        }
        if !manifest_ids.contains(&proposal.artifact_manifest_id) {
            return Err(ApiError::internal(
                "accepted Agent proposal references a missing same-paper artifact manifest",
            ));
        }
        let accepted_by = match decisions_by_proposal
            .get(&proposal.proposal_id)
            .map(Vec::as_slice)
        {
            Some([decision]) if decision.decision == HumanDecisionKind::Accept => {
                decision.player_id
            }
            _ => {
                return Err(ApiError::internal(
                    "accepted Agent proposal must have exactly one matching human acceptance",
                ));
            }
        };
        if !credited_manifest_ids.insert(proposal.artifact_manifest_id) {
            return Err(ApiError::conflict(
                "artifact_contribution_duplicated",
                "one accepted artifact manifest can be credited to only one author globally",
            ));
        }
        entries
            .get_mut(&accepted_by)
            .ok_or_else(|| {
                ApiError::internal(
                    "accepted Agent proposal was decided by a player outside the frozen author roster",
                )
            })?
            .accepted_artifact_manifest_ids
            .push(proposal.artifact_manifest_id);
    }

    let mut review_ids = HashSet::new();
    for review in section_reviews {
        if review.paper_project_id != paper_id || !review_ids.insert(review.review_id) {
            return Err(ApiError::internal(
                "authoritative contribution review set is cross-paper or duplicated",
            ));
        }
        if review.verdict != SectionReviewVerdict::Approve {
            continue;
        }
        entries
            .get_mut(&review.reviewer_player_id)
            .ok_or_else(|| {
                ApiError::internal(
                    "approving section review belongs to a player outside the frozen author roster",
                )
            })?
            .accepted_section_review_ids
            .push(review.review_id);
    }

    let mut result = entries.into_values().collect::<Vec<_>>();
    for entry in &mut result {
        entry.accepted_artifact_manifest_ids.sort();
        entry.accepted_artifact_manifest_ids.dedup();
        entry.accepted_section_review_ids.sort();
        entry.accepted_section_review_ids.dedup();
        entry.contribution_points = milestone_contribution_points(
            entry.accepted_artifact_manifest_ids.len(),
            entry.accepted_section_review_ids.len(),
        );
    }
    result.sort_by_key(|entry| entry.player_id);
    Ok(result)
}

pub(super) fn authoritative_contribution_entries_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    authors: &[crate::paper_raid_contracts::PaperReleaseAuthorV2],
) -> Result<Vec<CreditContribution>, ApiError> {
    let artifact_manifests = memory
        .collaboration
        .artifact_manifests
        .values()
        .filter(|manifest| manifest.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let proposals = memory
        .collaboration
        .proposals
        .values()
        .filter(|proposal| proposal.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let decisions = memory
        .collaboration
        .decisions
        .values()
        .filter(|decision| decision.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_reviews = memory
        .collaboration
        .section_reviews
        .values()
        .filter(|review| review.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    authoritative_contribution_entries_from_records(
        paper_id,
        authors,
        &artifact_manifests,
        &proposals,
        &decisions,
        &section_reviews,
    )
}

pub(super) async fn authoritative_contribution_entries_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    authors: &[crate::paper_raid_contracts::PaperReleaseAuthorV2],
) -> Result<Vec<CreditContribution>, ApiError> {
    let artifact_rows = sqlx::query(
        "select manifest_id,paper_project_id,manifest_hash,version,record_json
         from hepta_artifact_manifests where paper_project_id=$1 for share",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut artifact_manifests = Vec::with_capacity(artifact_rows.len());
    for row in artifact_rows {
        let manifest: ArtifactManifest =
            decode_record(row.get("record_json"), "artifact manifest")?;
        let version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("artifact manifest version is negative"))?;
        if manifest.manifest_id != row.get::<Uuid, _>("manifest_id")
            || manifest.paper_project_id != row.get::<Uuid, _>("paper_project_id")
            || manifest.manifest_hash != row.get::<String, _>("manifest_hash")
            || manifest.version != version
        {
            return Err(ApiError::internal(
                "artifact manifest relational columns disagree with record_json",
            ));
        }
        artifact_manifests.push(manifest);
    }

    let proposal_rows = sqlx::query(
        "select proposal_id,paper_project_id,artifact_manifest_id,status,version,record_json
         from hepta_agent_proposals where paper_project_id=$1 for share",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut proposals = Vec::with_capacity(proposal_rows.len());
    for row in proposal_rows {
        let proposal: AgentProposal = decode_record(row.get("record_json"), "Agent proposal")?;
        let version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("Agent proposal version is negative"))?;
        let status = row.get::<String, _>("status");
        let expected_status = match &proposal.status {
            AgentProposalStatus::Submitted => "submitted",
            AgentProposalStatus::Accepted => "accepted",
            AgentProposalStatus::Rework => "rework",
            AgentProposalStatus::Rejected => "rejected",
            AgentProposalStatus::Superseded => "superseded",
        };
        if proposal.proposal_id != row.get::<Uuid, _>("proposal_id")
            || proposal.paper_project_id != row.get::<Uuid, _>("paper_project_id")
            || proposal.artifact_manifest_id != row.get::<Uuid, _>("artifact_manifest_id")
            || expected_status != status
            || proposal.version != version
        {
            return Err(ApiError::internal(
                "Agent proposal relational columns disagree with record_json",
            ));
        }
        proposals.push(proposal);
    }

    let decision_rows = sqlx::query(
        "select decision_id,paper_project_id,proposal_id,player_id,decision,version,record_json
         from hepta_human_decisions where paper_project_id=$1 for share",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut decisions = Vec::with_capacity(decision_rows.len());
    for row in decision_rows {
        let decision: HumanDecision = decode_record(row.get("record_json"), "human decision")?;
        let version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("human decision version is negative"))?;
        let contract_decision = match &decision.decision {
            HumanDecisionKind::Accept => "accept",
            HumanDecisionKind::Rework => "rework",
            HumanDecisionKind::Reject => "reject",
        };
        if decision.decision_id != row.get::<Uuid, _>("decision_id")
            || decision.paper_project_id != row.get::<Uuid, _>("paper_project_id")
            || decision.proposal_id != row.get::<Uuid, _>("proposal_id")
            || decision.player_id != row.get::<Uuid, _>("player_id")
            || contract_decision != row.get::<String, _>("decision")
            || decision.version != version
        {
            return Err(ApiError::internal(
                "human decision relational columns disagree with record_json",
            ));
        }
        decisions.push(decision);
    }

    let review_rows = sqlx::query(
        "select review_id,paper_project_id,reviewer_player_id,verdict,version,record_json
         from hepta_section_reviews where paper_project_id=$1 for share",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut section_reviews = Vec::with_capacity(review_rows.len());
    for row in review_rows {
        let review: SectionReview = decode_record(row.get("record_json"), "section review")?;
        let version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("section review version is negative"))?;
        let verdict = match &review.verdict {
            SectionReviewVerdict::Approve => "approve",
            SectionReviewVerdict::Rework => "rework",
            SectionReviewVerdict::Reject => "reject",
        };
        if review.review_id != row.get::<Uuid, _>("review_id")
            || review.paper_project_id != row.get::<Uuid, _>("paper_project_id")
            || review.reviewer_player_id != row.get::<Uuid, _>("reviewer_player_id")
            || verdict != row.get::<String, _>("verdict")
            || review.version != version
        {
            return Err(ApiError::internal(
                "section review relational columns disagree with record_json",
            ));
        }
        section_reviews.push(review);
    }

    authoritative_contribution_entries_from_records(
        paper_id,
        authors,
        &artifact_manifests,
        &proposals,
        &decisions,
        &section_reviews,
    )
}

fn require_complete_authoritative_contribution_entries(
    submitted: &[CreditContribution],
    authoritative: &[CreditContribution],
) -> Result<(), ApiError> {
    if submitted != authoritative {
        return Err(ApiError::conflict(
            "contribution_ledger_entries_mismatch",
            "submitted contribution entries must exactly equal the complete authoritative milestone set",
        ));
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
    validate_contribution_ledger_id(request.contribution_ledger_id)?;
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
        require_contribution_ledger_reservation_memory(
            &next,
            request.contribution_ledger_id,
            paper_id,
            &request.release_candidate_hash,
        )?;
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
        let authoritative = authoritative_contribution_entries_memory(
            &next,
            paper_id,
            &context.release_candidate.authors,
        )?;
        require_complete_authoritative_contribution_entries(&entries, &authoritative)?;
        let ledger = make_contribution_ledger(&context, &request, authoritative, Utc::now())?;
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
    require_contribution_ledger_reservation_postgres(
        &mut tx,
        request.contribution_ledger_id,
        paper_id,
        &request.release_candidate_hash,
    )
    .await?;
    let entries = contribution_entry_skeletons(&context, &request.entries)?;
    let authoritative = authoritative_contribution_entries_postgres(
        &mut tx,
        paper_id,
        &context.release_candidate.authors,
    )
    .await?;
    require_complete_authoritative_contribution_entries(&entries, &authoritative)?;
    let storage_now = postgres_contribution_ledger_now(&mut tx).await?;
    let ledger = make_contribution_ledger(&context, &request, authoritative, storage_now)?;
    let result = sqlx::query(
        "insert into hepta_paper_contribution_ledgers
         (contribution_ledger_id,paper_project_id,release_candidate_hash,ledger_hash,
          version,entries_json,record_json,created_at)
         values ($1,$2,$3,$4,1,$5::jsonb,$6::jsonb,$7)",
    )
    .bind(ledger.contribution_ledger_id)
    .bind(paper_id)
    .bind(&ledger.release_candidate_hash)
    .bind(&ledger.ledger_hash)
    .bind(
        serde_json::to_value(&ledger.entries)
            .map_err(|error| ApiError::internal(format!("encode contribution entries: {error}")))?,
    )
    .bind(
        serde_json::to_value(&ledger)
            .map_err(|error| ApiError::internal(format!("encode contribution ledger: {error}")))?,
    )
    .bind(ledger.created_at)
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
    let ledger = memory
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
        })?;
    require_contribution_ledger_reservation_memory(
        memory,
        ledger.contribution_ledger_id,
        ledger.paper_project_id,
        &ledger.release_candidate_hash,
    )?;
    let entries = serde_json::to_value(&ledger.entries)
        .map_err(|error| ApiError::internal(format!("encode contribution entries: {error}")))?;
    validate_loaded_contribution_ledger(
        ledger.clone(),
        ledger.contribution_ledger_id,
        ledger.paper_project_id,
        &ledger.release_candidate_hash,
        &ledger.ledger_hash,
        &entries,
        ledger.version,
        ledger.created_at,
    )
}

async fn load_contribution_ledger_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<ContributionLedger, ApiError> {
    let row = sqlx::query(
        "select contribution_ledger_id,paper_project_id,release_candidate_hash,ledger_hash,
                entries_json,version,created_at,record_json
         from hepta_paper_contribution_ledgers
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
    let ledger: ContributionLedger = decode_record(row.get("record_json"), "contribution ledger")?;
    let contribution_ledger_id: Uuid = row.get("contribution_ledger_id");
    let relational_paper_id: Uuid = row.get("paper_project_id");
    let relational_release_hash: String = row.get("release_candidate_hash");
    require_contribution_ledger_reservation_postgres(
        tx,
        contribution_ledger_id,
        relational_paper_id,
        &relational_release_hash,
    )
    .await?;
    let version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("contribution ledger version is negative"))?;
    validate_loaded_contribution_ledger(
        ledger,
        contribution_ledger_id,
        relational_paper_id,
        &relational_release_hash,
        &row.get::<String, _>("ledger_hash"),
        &row.get::<serde_json::Value, _>("entries_json"),
        version,
        row.get("created_at"),
    )
}

fn draft_request_as_evaluation_request(
    request: &CreatePaperEvaluationDraftRequest,
    reviewer_attestations: Vec<ReviewAttestationRequest>,
) -> CreatePaperEvaluationRequest {
    CreatePaperEvaluationRequest {
        evaluation_id: request.evaluation_id,
        submission_id: request.submission_id,
        supersedes_evaluation_id: request.supersedes_evaluation_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: request.paper_bundle_hash.clone(),
        tolerance_policy: request.tolerance_policy.clone(),
        reference_metrics_micros: request.reference_metrics_micros.clone(),
        score_components: request.score_components.clone(),
        hard_gates: request.hard_gates.clone(),
        evaluator_player_id: request.evaluator_player_id,
        evaluator_signing_key_id: request.evaluator_signing_key_id.clone(),
        evaluator_signing_public_key: request.evaluator_signing_public_key.clone(),
        evaluator_signing_public_key_hash: request.evaluator_signing_public_key_hash.clone(),
        evaluator_coi_attestation_hash: request.evaluator_coi_attestation_hash.clone(),
        evaluator_signed_at_unix: request.evaluator_signed_at_unix,
        evaluator_signature: request.evaluator_signature.clone(),
        reviewer_attestations,
        idempotency_key: request.idempotency_key.clone(),
    }
}

fn stored_draft_as_evaluation_request(
    draft: &PaperEvaluationDraft,
    attestations: &[EvaluationDraftAttestation],
    idempotency_key: &str,
) -> CreatePaperEvaluationRequest {
    CreatePaperEvaluationRequest {
        evaluation_id: draft.evaluation_id,
        submission_id: draft.submission_id,
        supersedes_evaluation_id: draft.supersedes_evaluation_id,
        release_candidate_hash: draft.release_candidate_hash.clone(),
        paper_bundle_hash: draft.paper_bundle_hash.clone(),
        tolerance_policy: draft.tolerance_policy.clone(),
        reference_metrics_micros: draft.reference_metrics_micros.clone(),
        score_components: draft.paper_score.components.clone(),
        hard_gates: draft.paper_score.hard_gates.clone(),
        evaluator_player_id: draft.evaluator_player_id,
        evaluator_signing_key_id: draft.evaluator_signing_key_id.clone(),
        evaluator_signing_public_key: draft.evaluator_signing_public_key.clone(),
        evaluator_signing_public_key_hash: draft.evaluator_signing_public_key_hash.clone(),
        evaluator_coi_attestation_hash: draft.evaluator_coi_attestation_hash.clone(),
        evaluator_signed_at_unix: draft.evaluator_signed_at_unix,
        evaluator_signature: draft.evaluator_signature.clone(),
        reviewer_attestations: attestations
            .iter()
            .map(|stored| ReviewAttestationRequest {
                attestation_id: stored.attestation.attestation_id,
                reviewer_player_id: stored.attestation.reviewer_player_id,
                verdict: stored.attestation.verdict.clone(),
                signing_key_id: stored.attestation.signing_key_id.clone(),
                signing_public_key: stored.attestation.signing_public_key.clone(),
                signing_public_key_hash: stored.attestation.signing_public_key_hash.clone(),
                coi_attestation_hash: stored.attestation.coi_attestation_hash.clone(),
                signed_at_unix: stored.attestation.signed_at_unix,
                signature: stored.attestation.signature.clone(),
            })
            .collect(),
        idempotency_key: idempotency_key.to_string(),
    }
}

fn validate_evaluation_context(
    context: &ReviewPaperContext,
    submission: &JointPaperSubmission,
    ledger: &ContributionLedger,
    request: &CreatePaperEvaluationRequest,
    now: DateTime<Utc>,
    require_panel: bool,
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
    if require_panel && request.reviewer_attestations.len() != 2 {
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
    if authors.contains(&request.evaluator_player_id) {
        return Err(ApiError::forbidden(
            "review_panel_not_independent",
            "evaluator must be a non-author",
        ));
    }
    if require_panel
        && (reviewers.len() != 2
            || reviewers.contains(&request.evaluator_player_id)
            || reviewers.iter().any(|reviewer| authors.contains(reviewer)))
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
    let quality_gate_passed =
        evaluation.status == PaperEvaluationStatus::Accepted && evaluation.paper_score.eligible;
    let player_xp: BTreeMap<_, _> = ledger
        .entries
        .iter()
        .map(|entry| {
            let points = if quality_gate_passed {
                ACCEPTED_AUTHOR_RAID_BASE_XP
                    .checked_add(entry.contribution_points)
                    .ok_or_else(|| ApiError::internal("player XP overflow"))?
            } else {
                0
            };
            Ok((entry.player_id, points))
        })
        .collect::<Result<BTreeMap<_, _>, ApiError>>()?;
    let team_xp = player_xp
        .values()
        .try_fold(0_u64, |total, points| total.checked_add(*points).ok_or(()))
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

fn evaluation_draft_hash(draft: &PaperEvaluationDraft) -> Result<String, ApiError> {
    let immutable_record = match draft.schema.as_str() {
        EVALUATION_DRAFT_SCHEMA_V1 => json!({
            "schema":EVALUATION_DRAFT_SCHEMA_V1,
            "evaluation_id":draft.evaluation_id,
            "paper_project_id":draft.paper_project_id,
            "submission_id":draft.submission_id,
            "review_round":draft.review_round,
            "supersedes_evaluation_id":draft.supersedes_evaluation_id,
            "release_candidate_hash":draft.release_candidate_hash,
            "paper_bundle_hash":draft.paper_bundle_hash,
            "tolerance_policy":draft.tolerance_policy,
            "tolerance_policy_hash":draft.tolerance_policy_hash,
            "reference_metrics_micros":draft.reference_metrics_micros,
            "paper_score":draft.paper_score,
            "evaluator_player_id":draft.evaluator_player_id,
            "evaluator_signing_key_id":draft.evaluator_signing_key_id,
            "evaluator_signing_public_key":draft.evaluator_signing_public_key,
            "evaluator_signing_public_key_hash":draft.evaluator_signing_public_key_hash,
            "evaluator_coi_attestation_hash":draft.evaluator_coi_attestation_hash,
            "evaluator_signed_at_unix":draft.evaluator_signed_at_unix,
            "evaluator_signature":draft.evaluator_signature,
            "evaluation_signing_hash":draft.evaluation_signing_hash,
        }),
        EVALUATION_DRAFT_SCHEMA_V2 => json!({
            "schema":EVALUATION_DRAFT_SCHEMA_V2,
            "evaluation_id":draft.evaluation_id,
            "paper_project_id":draft.paper_project_id,
            "submission_id":draft.submission_id,
            "review_round":draft.review_round,
            "supersedes_evaluation_id":draft.supersedes_evaluation_id,
            "release_candidate_hash":draft.release_candidate_hash,
            "paper_bundle_hash":draft.paper_bundle_hash,
            "tolerance_policy":draft.tolerance_policy,
            "tolerance_policy_hash":draft.tolerance_policy_hash,
            "reference_metrics_micros":draft.reference_metrics_micros,
            "paper_score":draft.paper_score,
            "evaluator_player_id":draft.evaluator_player_id,
            "evaluator_signing_key_id":draft.evaluator_signing_key_id,
            "evaluator_signing_public_key":draft.evaluator_signing_public_key,
            "evaluator_signing_public_key_hash":draft.evaluator_signing_public_key_hash,
            "evaluator_coi_attestation_hash":draft.evaluator_coi_attestation_hash,
            "evaluator_signed_at_unix":draft.evaluator_signed_at_unix,
            "evaluator_signature":draft.evaluator_signature,
            "evaluation_signing_hash":draft.evaluation_signing_hash,
            "expires_at":draft.expires_at,
        }),
        _ => {
            return Err(ApiError::internal(
                "evaluation draft uses an unsupported schema version",
            ));
        }
    };
    record_hash(&immutable_record, "evaluation draft")
}

fn make_evaluation_draft(
    context: &ReviewPaperContext,
    request: &CreatePaperEvaluationDraftRequest,
    prepared: PreparedEvaluation,
    review_round: u64,
    now: DateTime<Utc>,
) -> Result<PaperEvaluationDraft, ApiError> {
    let expires_at = now
        .checked_add_signed(chrono::Duration::hours(EVALUATION_DRAFT_LEASE_HOURS))
        .ok_or_else(|| ApiError::internal("evaluation draft lease expiry overflow"))?;
    let mut draft = PaperEvaluationDraft {
        schema: EVALUATION_DRAFT_SCHEMA_V2.to_string(),
        evaluation_id: request.evaluation_id,
        paper_project_id: context.paper.paper_project_id,
        submission_id: request.submission_id,
        review_round,
        supersedes_evaluation_id: request.supersedes_evaluation_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: request.paper_bundle_hash.clone(),
        tolerance_policy: request.tolerance_policy.clone(),
        tolerance_policy_hash: prepared.tolerance_policy_hash,
        reference_metrics_micros: request.reference_metrics_micros.clone(),
        paper_score: prepared.paper_score,
        evaluator_player_id: request.evaluator_player_id,
        evaluator_signing_key_id: request.evaluator_signing_key_id.clone(),
        evaluator_signing_public_key: request.evaluator_signing_public_key.clone(),
        evaluator_signing_public_key_hash: request.evaluator_signing_public_key_hash.clone(),
        evaluator_coi_attestation_hash: request.evaluator_coi_attestation_hash.clone(),
        evaluator_signed_at_unix: request.evaluator_signed_at_unix,
        evaluator_signature: request.evaluator_signature.clone(),
        evaluation_signing_hash: prepared.evaluation_signing_hash,
        draft_hash: String::new(),
        status: EvaluationDraftStatus::Open,
        version: 1,
        finalized_evaluation_id: None,
        created_at: now,
        expires_at,
        updated_at: now,
        finalized_at: None,
        expired_at: None,
    };
    draft.draft_hash = evaluation_draft_hash(&draft)?;
    Ok(draft)
}

fn ensure_evaluation_draft_lease_live(
    draft: &PaperEvaluationDraft,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    match draft.status {
        EvaluationDraftStatus::Open if draft.expires_at > now => Ok(()),
        EvaluationDraftStatus::Open | EvaluationDraftStatus::Expired => Err(ApiError::conflict(
            "evaluation_draft_lease_expired",
            "evaluation draft lease expired; its pinned assignments must be reassigned through the Review Queue",
        )),
        EvaluationDraftStatus::Finalized => Err(ApiError::conflict(
            "evaluation_draft_finalized",
            "finalized evaluation drafts are immutable",
        )),
    }
}

fn expire_evaluation_draft(
    draft: &mut PaperEvaluationDraft,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    if draft.status != EvaluationDraftStatus::Open || draft.expires_at > now {
        return Ok(false);
    }
    draft.status = EvaluationDraftStatus::Expired;
    draft.version = draft
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("evaluation draft version overflow"))?;
    draft.updated_at = draft.expires_at;
    draft.expired_at = Some(draft.expires_at);
    Ok(true)
}

fn expirable_pinned_assignment_ids(
    draft: &PaperEvaluationDraft,
    attestations: &[EvaluationDraftAttestation],
    assignments: &[ReviewAssignment],
) -> Result<Vec<Uuid>, ApiError> {
    let mut expected =
        HashSet::from([(ReviewAssignmentSlot::Evaluator, draft.evaluator_player_id)]);
    for record in attestations {
        if record.evaluation_id != draft.evaluation_id
            || record.paper_project_id != draft.paper_project_id
            || record.submission_id != draft.submission_id
            || record.review_round != draft.review_round
            || record.draft_hash != draft.draft_hash
            || record.attestation.attestation_id != record.attestation_id
            || record.attestation.evaluation_id != draft.evaluation_id
            || record.attestation.evaluation_signing_hash != draft.evaluation_signing_hash
            || !matches!(
                record.slot,
                ReviewAssignmentSlot::Reviewer1 | ReviewAssignmentSlot::Reviewer2
            )
            || !expected.insert((record.slot, record.attestation.reviewer_player_id))
        {
            return Err(ApiError::internal(
                "expired evaluation draft has inconsistent immutable attestations",
            ));
        }
    }

    let pinned = assignments
        .iter()
        .filter(|assignment| {
            assignment.paper_project_id == draft.paper_project_id
                && assignment.submission_id == draft.submission_id
                && assignment.review_round == draft.review_round
                && assignment.status == ReviewAssignmentStatus::Pinned
                && assignment.pinned_evaluation_id == Some(draft.evaluation_id)
        })
        .collect::<Vec<_>>();
    if pinned.len() != expected.len()
        || pinned
            .iter()
            .any(|assignment| !expected.contains(&(assignment.slot, assignment.player_id)))
        || expected.iter().any(|(slot, player_id)| {
            !pinned
                .iter()
                .any(|assignment| assignment.slot == *slot && assignment.player_id == *player_id)
        })
    {
        return Err(ApiError::internal(
            "expired evaluation draft does not exactly own its pinned panel assignments",
        ));
    }
    let mut ids = pinned
        .into_iter()
        .map(|assignment| assignment.assignment_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    Ok(ids)
}

fn project_evaluation_draft_expirations(
    drafts: &mut [PaperEvaluationDraft],
    assignments: &mut [ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let mut stale_scopes = HashMap::new();
    for draft in drafts {
        if expire_evaluation_draft(draft, now)? {
            stale_scopes.insert(
                (
                    draft.paper_project_id,
                    draft.submission_id,
                    draft.review_round,
                ),
                (draft.evaluation_id, draft.expires_at),
            );
        }
    }
    for assignment in assignments.iter_mut().filter(|assignment| {
        assignment.status == ReviewAssignmentStatus::Pinned
            && stale_scopes
                .get(&(
                    assignment.paper_project_id,
                    assignment.submission_id,
                    assignment.review_round,
                ))
                .is_some_and(|(evaluation_id, _)| {
                    assignment.pinned_evaluation_id == Some(*evaluation_id)
                })
    }) {
        let expired_at = stale_scopes
            .get(&(
                assignment.paper_project_id,
                assignment.submission_id,
                assignment.review_round,
            ))
            .map(|(_, expired_at)| *expired_at)
            .ok_or_else(|| ApiError::internal("projected draft expiry scope disappeared"))?;
        transition_review_assignment(
            assignment,
            ReviewAssignmentStatus::Expired,
            None,
            expired_at,
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn age_evaluation_draft_for_recovery_test(
    draft: &mut PaperEvaluationDraft,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    draft.expires_at = draft
        .created_at
        .checked_add_signed(chrono::Duration::microseconds(1))
        .ok_or_else(|| ApiError::internal("test draft lease overflow"))?;
    if draft.expires_at >= now {
        return Err(ApiError::internal(
            "test clock did not advance beyond the synthetic draft deadline",
        ));
    }
    draft.draft_hash = evaluation_draft_hash(draft)?;
    Ok(())
}

#[cfg(test)]
pub(super) fn age_memory_evaluation_draft_for_recovery_test(
    memory: &mut PaperRaidMemory,
    evaluation_id: Uuid,
    now: DateTime<Utc>,
) -> Result<PaperEvaluationDraft, ApiError> {
    let draft = memory
        .review
        .evaluation_drafts
        .get_mut(&evaluation_id)
        .ok_or_else(|| ApiError::internal("test evaluation draft does not exist"))?;
    age_evaluation_draft_for_recovery_test(draft, now)?;
    Ok(draft.clone())
}

fn exact_active_assignment<'a>(
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    player_id: Uuid,
    slot: ReviewAssignmentSlot,
    mut assignments: impl Iterator<Item = &'a ReviewAssignment>,
    now: DateTime<Utc>,
) -> Result<&'a ReviewAssignment, ApiError> {
    assignments
        .find(|assignment| {
            assignment.paper_project_id == paper_id
                && assignment.submission_id == submission_id
                && assignment.review_round == review_round
                && assignment.player_id == player_id
                && assignment.slot == slot
                && review_assignment_active(assignment, now)
        })
        .ok_or_else(|| {
            ApiError::forbidden(
                "evaluation_draft_assignment_mismatch",
                "actor must hold the exact active Review Raid assignment for this draft round",
            )
        })
}

fn active_reviewer_assignment<'a>(
    draft: &PaperEvaluationDraft,
    player_id: Uuid,
    assignments: impl Iterator<Item = &'a ReviewAssignment>,
    now: DateTime<Utc>,
) -> Result<&'a ReviewAssignment, ApiError> {
    let matching = assignments
        .filter(|assignment| {
            assignment.paper_project_id == draft.paper_project_id
                && assignment.submission_id == draft.submission_id
                && assignment.review_round == draft.review_round
                && assignment.player_id == player_id
                && matches!(
                    assignment.slot,
                    ReviewAssignmentSlot::Reviewer1 | ReviewAssignmentSlot::Reviewer2
                )
                && assignment.pinned_evaluation_id.is_none()
                && review_assignment_active(assignment, now)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(ApiError::forbidden(
            "evaluation_draft_reviewer_assignment_mismatch",
            "attestation requires exactly one active reviewer_1 or reviewer_2 assignment",
        ));
    }
    Ok(matching[0])
}

fn evaluation_draft_quorum(
    draft: PaperEvaluationDraft,
    mut attestations: Vec<EvaluationDraftAttestation>,
    assignments: &[ReviewAssignment],
    now: DateTime<Utc>,
) -> Result<EvaluationDraftQuorum, ApiError> {
    let mut projected_drafts = vec![draft];
    let mut projected_assignments = assignments.to_vec();
    project_evaluation_draft_expirations(&mut projected_drafts, &mut projected_assignments, now)?;
    let draft = projected_drafts
        .pop()
        .ok_or_else(|| ApiError::internal("evaluation draft projection disappeared"))?;
    attestations.sort_by_key(|record| record.slot.rank());
    let required_slots = vec![
        ReviewAssignmentSlot::Reviewer1,
        ReviewAssignmentSlot::Reviewer2,
    ];
    let present: HashSet<_> = attestations.iter().map(|record| record.slot).collect();
    let missing_slots = required_slots
        .iter()
        .copied()
        .filter(|slot| !present.contains(slot))
        .collect::<Vec<_>>();
    let assignments_active = [
        (draft.evaluator_player_id, ReviewAssignmentSlot::Evaluator),
        (
            attestations
                .iter()
                .find(|record| record.slot == ReviewAssignmentSlot::Reviewer1)
                .map(|record| record.attestation.reviewer_player_id)
                .unwrap_or(Uuid::nil()),
            ReviewAssignmentSlot::Reviewer1,
        ),
        (
            attestations
                .iter()
                .find(|record| record.slot == ReviewAssignmentSlot::Reviewer2)
                .map(|record| record.attestation.reviewer_player_id)
                .unwrap_or(Uuid::nil()),
            ReviewAssignmentSlot::Reviewer2,
        ),
    ]
    .into_iter()
    .all(|(player_id, slot)| {
        projected_assignments.iter().any(|assignment| {
            assignment.paper_project_id == draft.paper_project_id
                && assignment.submission_id == draft.submission_id
                && assignment.review_round == draft.review_round
                && assignment.player_id == player_id
                && assignment.slot == slot
                && assignment.pinned_evaluation_id == Some(draft.evaluation_id)
                && review_assignment_active(assignment, now)
        })
    });
    let ready_to_finalize = draft.status == EvaluationDraftStatus::Open
        && missing_slots.is_empty()
        && assignments_active;
    Ok(EvaluationDraftQuorum {
        schema: EVALUATION_DRAFT_QUORUM_SCHEMA_V1.to_string(),
        draft,
        attestations,
        required_slots,
        missing_slots,
        assignments_active,
        ready_to_finalize,
    })
}

fn ensure_evaluation_draft_slot_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    evaluation_id: Uuid,
) -> Result<(), ApiError> {
    if memory.review.evaluations.contains_key(&evaluation_id)
        || memory.review.evaluation_drafts.values().any(|draft| {
            draft.evaluation_id == evaluation_id
                || (draft.paper_project_id == paper_id
                    && draft.submission_id == submission_id
                    && draft.review_round == review_round
                    && draft.status != EvaluationDraftStatus::Expired)
        })
    {
        return Err(ApiError::conflict(
            "evaluation_draft_identity_or_round_exists",
            "evaluation identity or submission review round is already reserved",
        ));
    }
    Ok(())
}

async fn lock_evaluation_round_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    review_round: u64,
) -> Result<(), ApiError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("hepta:evaluation-round:{paper_id}:{review_round}"))
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    Ok(())
}

async fn lock_evaluation_identity_postgres(
    tx: &mut Transaction<'_, Postgres>,
    evaluation_id: Uuid,
) -> Result<(), ApiError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("hepta:evaluation-identity:{evaluation_id}"))
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    Ok(())
}

async fn ensure_evaluation_draft_slot_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    evaluation_id: Uuid,
) -> Result<(), ApiError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "select exists(
           select 1 from hepta_paper_evaluation_drafts
           where evaluation_id=$4
              or (paper_project_id=$1 and submission_id=$2 and review_round=$3
                  and status in ('open','finalized'))
           union all
           select 1 from hepta_paper_evaluations where evaluation_id=$4
         )",
    )
    .bind(paper_id)
    .bind(submission_id)
    .bind(i64::try_from(review_round).map_err(|_| ApiError::internal("review round overflow"))?)
    .bind(evaluation_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if exists {
        return Err(ApiError::conflict(
            "evaluation_draft_identity_or_round_exists",
            "evaluation identity or submission review round is already reserved",
        ));
    }
    Ok(())
}

fn decode_evaluation_draft_row(row: &PgRow) -> Result<PaperEvaluationDraft, ApiError> {
    decode_record(row.get("record_json"), "evaluation draft")
}

async fn load_evaluation_draft_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    evaluation_id: Uuid,
    lock: bool,
) -> Result<PaperEvaluationDraft, ApiError> {
    let suffix = if lock { " for update" } else { " for share" };
    let row = sqlx::query(&format!(
        "select record_json from hepta_paper_evaluation_drafts
         where paper_project_id=$1 and evaluation_id=$2{suffix}"
    ))
    .bind(paper_id)
    .bind(evaluation_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "evaluation_draft_not_found",
            "evaluation draft does not exist for this paper",
        )
    })?;
    decode_evaluation_draft_row(&row)
}

async fn load_draft_attestations_postgres(
    tx: &mut Transaction<'_, Postgres>,
    evaluation_id: Uuid,
) -> Result<Vec<EvaluationDraftAttestation>, ApiError> {
    let rows = sqlx::query(
        "select record_json from hepta_paper_evaluation_draft_attestations
         where evaluation_id=$1 order by slot for share",
    )
    .bind(evaluation_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    rows.iter()
        .map(|row| decode_record(row.get("record_json"), "evaluation draft attestation"))
        .collect()
}

async fn create_paper_evaluation_draft(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperEvaluationDraftRequest>,
) -> Result<(StatusCode, Json<PaperEvaluationDraft>), ApiError> {
    const OPERATION: &str = "create_paper_evaluation_draft_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest("release_candidate_hash", &request.release_candidate_hash)?;
    validate_digest("paper_bundle_hash", &request.paper_bundle_hash)?;
    validate_digest(
        "evaluator_coi_attestation_hash",
        &request.evaluator_coi_attestation_hash,
    )?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evaluation-drafts");
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
            "evaluation_draft_evaluator_assertion_mismatch",
            "Consumer assertion must identify the assigned evaluator",
        ));
    }
    let now = Utc::now();
    let evaluation_request = draft_request_as_evaluation_request(&request, Vec::new());

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &body_hash)?
        {
            return Ok(replay);
        }
        let _finality_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_guard,
            paper_id,
        )?;
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let submission = load_submission_memory(&next, paper_id, request.submission_id)?;
        let ledger =
            load_contribution_ledger_memory(&next, paper_id, &request.release_candidate_hash)?;
        let review_round = validate_supersession_memory(&next, paper_id, &evaluation_request)?;
        ensure_evaluation_draft_slot_memory(
            &next,
            paper_id,
            submission.submission_id,
            review_round,
            request.evaluation_id,
        )?;
        let evaluator_assignment_id = exact_active_assignment(
            paper_id,
            submission.submission_id,
            review_round,
            request.evaluator_player_id,
            ReviewAssignmentSlot::Evaluator,
            next.review.assignments.values(),
            now,
        )?
        .assignment_id;
        let prepared = validate_evaluation_context(
            &context,
            &submission,
            &ledger,
            &evaluation_request,
            now,
            false,
        )?;
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
            "evaluation draft evaluator signature verification failed",
        )?;
        let draft = make_evaluation_draft(&context, &request, prepared, review_round, now)?;
        if next
            .review
            .evaluation_drafts
            .insert(draft.evaluation_id, draft.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "evaluation_draft_exists",
                "evaluation_id already identifies an immutable draft",
            ));
        }
        transition_review_assignment_memory(
            &mut next.review,
            evaluator_assignment_id,
            ReviewAssignmentStatus::Pinned,
            Some(draft.evaluation_id),
            now,
        )?;
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.evaluation_draft.created.v1",
            paper_id,
            paper_id,
            review_round,
            json!({"evaluation_id":draft.evaluation_id,"review_round":review_round,"draft_hash":draft.draft_hash,"evaluation_signing_hash":draft.evaluation_signing_hash,"lease_expires_at":draft.expires_at,"pinned_evaluator_assignment_id":evaluator_assignment_id}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &draft,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(draft)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission = load_submission_postgres(&mut tx, paper_id, request.submission_id).await?;
    let ledger =
        load_contribution_ledger_postgres(&mut tx, paper_id, &request.release_candidate_hash)
            .await?;
    let candidate_round =
        validate_supersession_postgres(&mut tx, paper_id, &evaluation_request).await?;
    lock_evaluation_round_postgres(&mut tx, paper_id, candidate_round).await?;
    lock_evaluation_identity_postgres(&mut tx, request.evaluation_id).await?;
    let review_round =
        validate_supersession_postgres(&mut tx, paper_id, &evaluation_request).await?;
    if review_round != candidate_round {
        return Err(ApiError::conflict(
            "evaluation_round_changed",
            "review round changed while the evaluation draft was being created",
        ));
    }
    ensure_evaluation_draft_slot_postgres(
        &mut tx,
        paper_id,
        submission.submission_id,
        review_round,
        request.evaluation_id,
    )
    .await?;
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    let evaluator_assignment_id = exact_active_assignment(
        paper_id,
        submission.submission_id,
        review_round,
        request.evaluator_player_id,
        ReviewAssignmentSlot::Evaluator,
        assignments.iter(),
        now,
    )?
    .assignment_id;
    let prepared = validate_evaluation_context(
        &context,
        &submission,
        &ledger,
        &evaluation_request,
        now,
        false,
    )?;
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
        "evaluation draft evaluator signature verification failed",
    )?;
    let draft = make_evaluation_draft(&context, &request, prepared, review_round, now)?;
    let inserted = sqlx::query(
        "insert into hepta_paper_evaluation_drafts (
            evaluation_id,paper_project_id,submission_id,review_round,
            supersedes_evaluation_id,evaluator_player_id,draft_hash,
            evaluation_signing_hash,status,version,record_json,created_at,expires_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,'open',1,$9::jsonb,$10,$11,$10)",
    )
    .bind(draft.evaluation_id)
    .bind(paper_id)
    .bind(draft.submission_id)
    .bind(i64::try_from(review_round).map_err(|_| ApiError::internal("review round overflow"))?)
    .bind(draft.supersedes_evaluation_id)
    .bind(draft.evaluator_player_id)
    .bind(&draft.draft_hash)
    .bind(&draft.evaluation_signing_hash)
    .bind(
        serde_json::to_value(&draft)
            .map_err(|error| ApiError::internal(format!("encode evaluation draft: {error}")))?,
    )
    .bind(now)
    .bind(draft.expires_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "evaluation_draft_exists",
                "evaluation identity or immutable review-round slot already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    transition_review_assignment_postgres(
        &mut tx,
        evaluator_assignment_id,
        ReviewAssignmentStatus::Pinned,
        Some(draft.evaluation_id),
        now,
    )
    .await?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.evaluation_draft.created.v1",
        paper_id,
        paper_id,
        review_round,
        json!({"evaluation_id":draft.evaluation_id,"review_round":review_round,"draft_hash":draft.draft_hash,"evaluation_signing_hash":draft.evaluation_signing_hash,"lease_expires_at":draft.expires_at,"pinned_evaluator_assignment_id":evaluator_assignment_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(draft.evaluation_id),
        StatusCode::CREATED,
        &draft,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(draft)))
}

async fn get_paper_evaluation_draft(
    State(state): State<AppState>,
    Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<EvaluationDraftQuorum>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}");
    let assertion =
        require_member_read_assertion(&headers, &state, "get_paper_evaluation_draft_v1", &path)?;
    let now = Utc::now();
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        active_registered_player_memory(&memory, &assertion)?;
        let draft = memory
            .review
            .evaluation_drafts
            .get(&evaluation_id)
            .filter(|draft| draft.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "evaluation_draft_not_found",
                    "evaluation draft does not exist for this paper",
                )
            })?;
        let assignments = memory
            .review
            .assignments
            .values()
            .filter(|assignment| assignment.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut projected_drafts = vec![draft];
        let mut assignments = assignments;
        project_evaluation_draft_expirations(&mut projected_drafts, &mut assignments, now)?;
        let draft = projected_drafts
            .pop()
            .ok_or_else(|| ApiError::internal("evaluation draft projection disappeared"))?;
        if !assignments.iter().any(|assignment| {
            assignment.submission_id == draft.submission_id
                && assignment.review_round == draft.review_round
                && assignment.player_id == assertion.player_id
                && review_assignment_active(assignment, now)
        }) {
            return Err(ApiError::forbidden(
                "evaluation_draft_access_denied",
                "evaluation draft quorum is limited to actively assigned panel members",
            ));
        }
        let attestations = memory
            .review
            .draft_attestations
            .values()
            .filter(|record| record.evaluation_id == evaluation_id)
            .cloned()
            .collect();
        return Ok(Json(evaluation_draft_quorum(
            draft,
            attestations,
            &assignments,
            now,
        )?));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    active_registered_player_postgres(&mut tx, &assertion).await?;
    let draft = load_evaluation_draft_postgres(&mut tx, paper_id, evaluation_id, false).await?;
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    let mut projected_drafts = vec![draft];
    let mut assignments = assignments;
    project_evaluation_draft_expirations(&mut projected_drafts, &mut assignments, now)?;
    let draft = projected_drafts
        .pop()
        .ok_or_else(|| ApiError::internal("evaluation draft projection disappeared"))?;
    if !assignments.iter().any(|assignment| {
        assignment.submission_id == draft.submission_id
            && assignment.review_round == draft.review_round
            && assignment.player_id == assertion.player_id
            && review_assignment_active(assignment, now)
    }) {
        return Err(ApiError::forbidden(
            "evaluation_draft_access_denied",
            "evaluation draft quorum is limited to actively assigned panel members",
        ));
    }
    let attestations = load_draft_attestations_postgres(&mut tx, evaluation_id).await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(evaluation_draft_quorum(
        draft,
        attestations,
        &assignments,
        now,
    )?))
}

fn draft_attestation_request(
    request: &SubmitEvaluationDraftAttestationRequest,
) -> ReviewAttestationRequest {
    ReviewAttestationRequest {
        attestation_id: request.attestation_id,
        reviewer_player_id: request.reviewer_player_id,
        verdict: request.verdict.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        coi_attestation_hash: request.coi_attestation_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
    }
}

async fn submit_evaluation_draft_attestation(
    State(state): State<AppState>,
    Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<SubmitEvaluationDraftAttestationRequest>,
) -> Result<(StatusCode, Json<EvaluationDraftAttestation>), ApiError> {
    const OPERATION: &str = "submit_paper_evaluation_draft_attestation_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest("draft_hash", &request.draft_hash)?;
    validate_digest(
        "reviewer_coi_attestation_hash",
        &request.coi_attestation_hash,
    )?;
    signed_time(request.signed_at_unix)?;
    let body_hash = request_hash(&request)?;
    let path =
        format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/attestations");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &body_hash,
    )?;
    if assertion.player_id != request.reviewer_player_id {
        return Err(ApiError::forbidden(
            "evaluation_draft_reviewer_assertion_mismatch",
            "Consumer assertion must identify the assigned reviewer",
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
        let _finality_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_guard,
            paper_id,
        )?;
        let mut next = memory.clone();
        let draft = next
            .review
            .evaluation_drafts
            .get(&evaluation_id)
            .filter(|draft| draft.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "evaluation_draft_not_found",
                    "evaluation draft does not exist for this paper",
                )
            })?;
        ensure_evaluation_draft_lease_live(&draft, now)?;
        if draft.draft_hash != request.draft_hash {
            return Err(ApiError::conflict(
                "evaluation_draft_hash_mismatch",
                "review attestation must bind the exact immutable draft hash",
            ));
        }
        if next.review.draft_attestations.values().any(|record| {
            record.evaluation_id == evaluation_id
                && (record.attestation.reviewer_player_id == request.reviewer_player_id
                    || record.attestation_id == request.attestation_id)
        }) {
            return Err(ApiError::conflict(
                "evaluation_draft_attestation_exists",
                "reviewer slot, reviewer, or attestation identity is already immutable",
            ));
        }
        let (assignment_id, slot) = {
            let assignment = active_reviewer_assignment(
                &draft,
                request.reviewer_player_id,
                next.review.assignments.values(),
                now,
            )?;
            (assignment.assignment_id, assignment.slot)
        };
        if next.review.draft_attestations.values().any(|record| {
            record.evaluation_id == evaluation_id
                && (record.slot == slot
                    || record.attestation.reviewer_player_id == request.reviewer_player_id)
        }) || next
            .review
            .draft_attestations
            .contains_key(&request.attestation_id)
        {
            return Err(ApiError::conflict(
                "evaluation_draft_attestation_exists",
                "reviewer slot, reviewer, or attestation identity is already immutable",
            ));
        }
        let (reviewer, key) = active_player_key_memory(
            &next,
            request.reviewer_player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &reviewer)?;
        let attestation = review_attestation(
            &draft_attestation_request(&request),
            evaluation_id,
            &draft.evaluation_signing_hash,
            &key,
        )?;
        let stored = EvaluationDraftAttestation {
            schema: EVALUATION_DRAFT_ATTESTATION_SCHEMA_V1.to_string(),
            attestation_id: request.attestation_id,
            evaluation_id,
            paper_project_id: paper_id,
            submission_id: draft.submission_id,
            review_round: draft.review_round,
            slot,
            draft_hash: draft.draft_hash.clone(),
            attestation,
            created_at: now,
        };
        next.review
            .draft_attestations
            .insert(stored.attestation_id, stored.clone());
        transition_review_assignment_memory(
            &mut next.review,
            assignment_id,
            ReviewAssignmentStatus::Pinned,
            Some(evaluation_id),
            now,
        )?;
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.evaluation_draft.attested.v1",
            paper_id,
            paper_id,
            draft.review_round,
            json!({"evaluation_id":evaluation_id,"attestation_id":stored.attestation_id,"slot":slot,"draft_hash":stored.draft_hash,"pinned_reviewer_assignment_id":assignment_id}),
        );
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            body_hash,
            StatusCode::CREATED,
            &stored,
        )?;
        *memory = next;
        return Ok((StatusCode::CREATED, Json(stored)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &body_hash).await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let draft = load_evaluation_draft_postgres(&mut tx, paper_id, evaluation_id, true).await?;
    ensure_evaluation_draft_lease_live(&draft, now)?;
    if draft.draft_hash != request.draft_hash {
        return Err(ApiError::conflict(
            "evaluation_draft_hash_mismatch",
            "review attestation must bind the exact immutable draft hash",
        ));
    }
    let immutable_conflict = sqlx::query_scalar::<_, bool>(
        "select exists(
           select 1 from hepta_paper_evaluation_draft_attestations
           where evaluation_id=$1
             and (reviewer_player_id=$2 or attestation_id=$3)
         )",
    )
    .bind(evaluation_id)
    .bind(request.reviewer_player_id)
    .bind(request.attestation_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if immutable_conflict {
        return Err(ApiError::conflict(
            "evaluation_draft_attestation_exists",
            "reviewer slot, reviewer, or attestation identity is already immutable",
        ));
    }
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    let (assignment_id, slot) = {
        let assignment = active_reviewer_assignment(
            &draft,
            request.reviewer_player_id,
            assignments.iter(),
            now,
        )?;
        (assignment.assignment_id, assignment.slot)
    };
    let (reviewer, key) = active_player_key_postgres(
        &mut tx,
        request.reviewer_player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &reviewer)?;
    let attestation = review_attestation(
        &draft_attestation_request(&request),
        evaluation_id,
        &draft.evaluation_signing_hash,
        &key,
    )?;
    let stored = EvaluationDraftAttestation {
        schema: EVALUATION_DRAFT_ATTESTATION_SCHEMA_V1.to_string(),
        attestation_id: request.attestation_id,
        evaluation_id,
        paper_project_id: paper_id,
        submission_id: draft.submission_id,
        review_round: draft.review_round,
        slot,
        draft_hash: draft.draft_hash.clone(),
        attestation,
        created_at: now,
    };
    let inserted = sqlx::query(
        "insert into hepta_paper_evaluation_draft_attestations (
            attestation_id,evaluation_id,paper_project_id,submission_id,review_round,
            slot,reviewer_player_id,draft_hash,evaluation_signing_hash,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11)",
    )
    .bind(stored.attestation_id)
    .bind(evaluation_id)
    .bind(paper_id)
    .bind(stored.submission_id)
    .bind(
        i64::try_from(stored.review_round)
            .map_err(|_| ApiError::internal("review round overflow"))?,
    )
    .bind(slot.as_str())
    .bind(stored.attestation.reviewer_player_id)
    .bind(&stored.draft_hash)
    .bind(&stored.attestation.evaluation_signing_hash)
    .bind(serde_json::to_value(&stored).map_err(|error| {
        ApiError::internal(format!("encode evaluation draft attestation: {error}"))
    })?)
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|database| database.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "evaluation_draft_attestation_exists",
                "reviewer slot, reviewer, or attestation identity is already immutable",
            ));
        }
        return Err(ApiError::database(error));
    }
    transition_review_assignment_postgres(
        &mut tx,
        assignment_id,
        ReviewAssignmentStatus::Pinned,
        Some(evaluation_id),
        now,
    )
    .await?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.evaluation_draft.attested.v1",
        paper_id,
        paper_id,
        draft.review_round,
        json!({"evaluation_id":evaluation_id,"attestation_id":stored.attestation_id,"slot":slot,"draft_hash":stored.draft_hash,"pinned_reviewer_assignment_id":assignment_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(stored.attestation_id),
        StatusCode::CREATED,
        &stored,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(stored)))
}

fn validate_draft_attestation_quorum(
    draft: &PaperEvaluationDraft,
    attestations: &[EvaluationDraftAttestation],
) -> Result<(), ApiError> {
    if attestations.len() != 2
        || attestations.iter().any(|record| {
            record.evaluation_id != draft.evaluation_id
                || record.paper_project_id != draft.paper_project_id
                || record.submission_id != draft.submission_id
                || record.review_round != draft.review_round
                || record.draft_hash != draft.draft_hash
                || record.attestation.evaluation_signing_hash != draft.evaluation_signing_hash
        })
    {
        return Err(ApiError::conflict(
            "evaluation_draft_quorum_incomplete",
            "finalization requires two attestations bound to the exact immutable draft",
        ));
    }
    let slots: HashSet<_> = attestations.iter().map(|record| record.slot).collect();
    let reviewers: HashSet<_> = attestations
        .iter()
        .map(|record| record.attestation.reviewer_player_id)
        .collect();
    if slots
        != HashSet::from([
            ReviewAssignmentSlot::Reviewer1,
            ReviewAssignmentSlot::Reviewer2,
        ])
        || reviewers.len() != 2
    {
        return Err(ApiError::conflict(
            "evaluation_draft_quorum_invalid",
            "quorum must contain exactly reviewer_1 and reviewer_2 from distinct players",
        ));
    }
    Ok(())
}

async fn finalize_paper_evaluation_draft(
    State(state): State<AppState>,
    Path((paper_id, evaluation_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<FinalizePaperEvaluationDraftRequest>,
) -> Result<(StatusCode, Json<PaperEvaluation>), ApiError> {
    const OPERATION: &str = "finalize_paper_evaluation_draft_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    let body_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/finalize");
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
        let _finality_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_guard,
            paper_id,
        )?;
        let mut next = memory.clone();
        let mut draft = next
            .review
            .evaluation_drafts
            .get(&evaluation_id)
            .filter(|draft| draft.paper_project_id == paper_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "evaluation_draft_not_found",
                    "evaluation draft does not exist for this paper",
                )
            })?;
        ensure_evaluation_draft_lease_live(&draft, now)?;
        if draft.version != request.expected_draft_version {
            return Err(ApiError::conflict(
                "evaluation_draft_version_conflict",
                "evaluation draft is finalized or changed from the expected version",
            ));
        }
        if assertion.player_id != draft.evaluator_player_id {
            return Err(ApiError::forbidden(
                "evaluation_draft_finalizer_mismatch",
                "only the assigned evaluator may finalize the draft",
            ));
        }
        let mut attestations = next
            .review
            .draft_attestations
            .values()
            .filter(|record| record.evaluation_id == evaluation_id)
            .cloned()
            .collect::<Vec<_>>();
        attestations.sort_by_key(|record| record.slot.rank());
        validate_draft_attestation_quorum(&draft, &attestations)?;
        let evaluation_request =
            stored_draft_as_evaluation_request(&draft, &attestations, &request.idempotency_key);
        let context = review_context_memory(&next, paper_id)?;
        let submission = load_submission_memory(&next, paper_id, draft.submission_id)?;
        let ledger =
            load_contribution_ledger_memory(&next, paper_id, &draft.release_candidate_hash)?;
        let review_round = validate_supersession_memory(&next, paper_id, &evaluation_request)?;
        if review_round != draft.review_round {
            return Err(ApiError::conflict(
                "evaluation_draft_round_stale",
                "Appeal or evaluation lineage changed after the draft was frozen",
            ));
        }
        let mut prepared = validate_evaluation_context(
            &context,
            &submission,
            &ledger,
            &evaluation_request,
            now,
            true,
        )?;
        if prepared.evaluation_signing_hash != draft.evaluation_signing_hash
            || evaluation_draft_hash(&draft)? != draft.draft_hash
        {
            return Err(ApiError::conflict(
                "evaluation_draft_integrity_failed",
                "frozen draft no longer derives its recorded signing or draft hash",
            ));
        }
        prepared.paper_score = draft.paper_score.clone();
        let assignments = next
            .review
            .assignments
            .values()
            .cloned()
            .collect::<Vec<_>>();
        enforce_evaluation_assignments(
            paper_id,
            submission.submission_id,
            review_round,
            &evaluation_request,
            assignments.iter(),
            now,
        )?;
        let panel_assignment_ids = pinned_evaluation_panel_assignment_ids(
            paper_id,
            submission.submission_id,
            review_round,
            &evaluation_request,
            &assignments,
        )?;
        let (evaluator, evaluator_key) = active_player_key_memory(
            &next,
            draft.evaluator_player_id,
            &draft.evaluator_signing_key_id,
            &draft.evaluator_signing_public_key,
            &draft.evaluator_signing_public_key_hash,
        )?;
        assert_player_identity(&assertion, &evaluator)?;
        verify_signature(
            &prepared.evaluator_frame,
            &draft.evaluator_signature,
            &evaluator_key,
            "evaluation_signature_failed",
            "evaluation draft evaluator signature verification failed",
        )?;
        let mut verified_attestations = Vec::with_capacity(2);
        for stored in &attestations {
            let (_, key) = active_player_key_memory(
                &next,
                stored.attestation.reviewer_player_id,
                &stored.attestation.signing_key_id,
                &stored.attestation.signing_public_key,
                &stored.attestation.signing_public_key_hash,
            )?;
            verified_attestations.push(review_attestation(
                &ReviewAttestationRequest {
                    attestation_id: stored.attestation.attestation_id,
                    reviewer_player_id: stored.attestation.reviewer_player_id,
                    verdict: stored.attestation.verdict.clone(),
                    signing_key_id: stored.attestation.signing_key_id.clone(),
                    signing_public_key: stored.attestation.signing_public_key.clone(),
                    signing_public_key_hash: stored.attestation.signing_public_key_hash.clone(),
                    coi_attestation_hash: stored.attestation.coi_attestation_hash.clone(),
                    signed_at_unix: stored.attestation.signed_at_unix,
                    signature: stored.attestation.signature.clone(),
                },
                evaluation_id,
                &draft.evaluation_signing_hash,
                &key,
            )?);
        }
        let evaluation = make_evaluation(
            &context,
            &evaluation_request,
            prepared,
            verified_attestations,
            now,
            review_round,
        );
        let raid_score = make_raid_score(&evaluation, &ledger, now)?;
        if next
            .review
            .evaluations
            .insert(evaluation_id, evaluation.clone())
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
        for assignment_id in &panel_assignment_ids {
            transition_review_assignment_memory(
                &mut next.review,
                *assignment_id,
                ReviewAssignmentStatus::Consumed,
                None,
                now,
            )?;
        }
        draft.status = EvaluationDraftStatus::Finalized;
        draft.version = draft
            .version
            .checked_add(1)
            .ok_or_else(|| ApiError::internal("evaluation draft version overflow"))?;
        draft.finalized_evaluation_id = Some(evaluation_id);
        draft.updated_at = now;
        draft.finalized_at = Some(now);
        next.review
            .evaluation_drafts
            .insert(evaluation_id, draft.clone());
        push_room_event_memory(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.evaluation.recorded.v1",
            paper_id,
            paper_id,
            evaluation.version,
            json!({"evaluation_id":evaluation_id,"status":evaluation.status,"settlement_state":evaluation.settlement_state,"paper_score_hash":evaluation.paper_score.score_hash}),
        );
        push_room_event_memory(
            &mut next,
            OPERATION,
            &format!("{}:draft", request.idempotency_key),
            "hepta.paper_raid.evaluation_draft.finalized.v1",
            paper_id,
            paper_id,
            draft.version,
            json!({"evaluation_id":evaluation_id,"review_round":draft.review_round,"draft_hash":draft.draft_hash,"consumed_panel_assignment_ids":panel_assignment_ids}),
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
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let mut draft = load_evaluation_draft_postgres(&mut tx, paper_id, evaluation_id, true).await?;
    ensure_evaluation_draft_lease_live(&draft, now)?;
    if draft.version != request.expected_draft_version {
        return Err(ApiError::conflict(
            "evaluation_draft_version_conflict",
            "evaluation draft is finalized or changed from the expected version",
        ));
    }
    if assertion.player_id != draft.evaluator_player_id {
        return Err(ApiError::forbidden(
            "evaluation_draft_finalizer_mismatch",
            "only the assigned evaluator may finalize the draft",
        ));
    }
    lock_evaluation_round_postgres(&mut tx, paper_id, draft.review_round).await?;
    let attestations = load_draft_attestations_postgres(&mut tx, evaluation_id).await?;
    validate_draft_attestation_quorum(&draft, &attestations)?;
    let evaluation_request =
        stored_draft_as_evaluation_request(&draft, &attestations, &request.idempotency_key);
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission = load_submission_postgres(&mut tx, paper_id, draft.submission_id).await?;
    let ledger =
        load_contribution_ledger_postgres(&mut tx, paper_id, &draft.release_candidate_hash).await?;
    let review_round =
        validate_supersession_postgres(&mut tx, paper_id, &evaluation_request).await?;
    if review_round != draft.review_round {
        return Err(ApiError::conflict(
            "evaluation_draft_round_stale",
            "Appeal or evaluation lineage changed after the draft was frozen",
        ));
    }
    let mut prepared = validate_evaluation_context(
        &context,
        &submission,
        &ledger,
        &evaluation_request,
        now,
        true,
    )?;
    if prepared.evaluation_signing_hash != draft.evaluation_signing_hash
        || evaluation_draft_hash(&draft)? != draft.draft_hash
    {
        return Err(ApiError::conflict(
            "evaluation_draft_integrity_failed",
            "frozen draft no longer derives its recorded signing or draft hash",
        ));
    }
    prepared.paper_score = draft.paper_score.clone();
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    enforce_evaluation_assignments(
        paper_id,
        submission.submission_id,
        review_round,
        &evaluation_request,
        assignments.iter(),
        now,
    )?;
    let panel_assignment_ids = pinned_evaluation_panel_assignment_ids(
        paper_id,
        submission.submission_id,
        review_round,
        &evaluation_request,
        &assignments,
    )?;
    let (evaluator, evaluator_key) = active_player_key_postgres(
        &mut tx,
        draft.evaluator_player_id,
        &draft.evaluator_signing_key_id,
        &draft.evaluator_signing_public_key,
        &draft.evaluator_signing_public_key_hash,
    )
    .await?;
    assert_player_identity(&assertion, &evaluator)?;
    verify_signature(
        &prepared.evaluator_frame,
        &draft.evaluator_signature,
        &evaluator_key,
        "evaluation_signature_failed",
        "evaluation draft evaluator signature verification failed",
    )?;
    let mut verified_attestations = Vec::with_capacity(2);
    for stored in &attestations {
        let (_, key) = active_player_key_postgres(
            &mut tx,
            stored.attestation.reviewer_player_id,
            &stored.attestation.signing_key_id,
            &stored.attestation.signing_public_key,
            &stored.attestation.signing_public_key_hash,
        )
        .await?;
        verified_attestations.push(review_attestation(
            &ReviewAttestationRequest {
                attestation_id: stored.attestation.attestation_id,
                reviewer_player_id: stored.attestation.reviewer_player_id,
                verdict: stored.attestation.verdict.clone(),
                signing_key_id: stored.attestation.signing_key_id.clone(),
                signing_public_key: stored.attestation.signing_public_key.clone(),
                signing_public_key_hash: stored.attestation.signing_public_key_hash.clone(),
                coi_attestation_hash: stored.attestation.coi_attestation_hash.clone(),
                signed_at_unix: stored.attestation.signed_at_unix,
                signature: stored.attestation.signature.clone(),
            },
            evaluation_id,
            &draft.evaluation_signing_hash,
            &key,
        )?);
    }
    let evaluation = make_evaluation(
        &context,
        &evaluation_request,
        prepared,
        verified_attestations,
        now,
        review_round,
    );
    let raid_score = make_raid_score(&evaluation, &ledger, now)?;
    insert_evaluation_postgres(&mut tx, &evaluation, &raid_score).await?;
    for assignment_id in &panel_assignment_ids {
        transition_review_assignment_postgres(
            &mut tx,
            *assignment_id,
            ReviewAssignmentStatus::Consumed,
            None,
            now,
        )
        .await?;
    }
    draft.status = EvaluationDraftStatus::Finalized;
    draft.version = draft
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("evaluation draft version overflow"))?;
    draft.finalized_evaluation_id = Some(evaluation_id);
    draft.updated_at = now;
    draft.finalized_at = Some(now);
    let updated = sqlx::query(
        "update hepta_paper_evaluation_drafts
         set status='finalized',version=$1,record_json=$2::jsonb,updated_at=$3,finalized_at=$3
         where evaluation_id=$4 and paper_project_id=$5 and status='open' and version=$6",
    )
    .bind(i64::try_from(draft.version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(
        serde_json::to_value(&draft)
            .map_err(|error| ApiError::internal(format!("encode evaluation draft: {error}")))?,
    )
    .bind(now)
    .bind(evaluation_id)
    .bind(paper_id)
    .bind(
        i64::try_from(request.expected_draft_version)
            .map_err(|_| ApiError::internal("version overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "evaluation_draft_version_conflict",
            "evaluation draft changed concurrently",
        ));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.evaluation.recorded.v1",
        paper_id,
        paper_id,
        evaluation.version,
        json!({"evaluation_id":evaluation_id,"status":evaluation.status,"settlement_state":evaluation.settlement_state,"paper_score_hash":evaluation.paper_score.score_hash}),
    )
    .await?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &format!("{}:draft", request.idempotency_key),
        "hepta.paper_raid.evaluation_draft.finalized.v1",
        paper_id,
        paper_id,
        draft.version,
        json!({"evaluation_id":evaluation_id,"review_round":draft.review_round,"draft_hash":draft.draft_hash,"consumed_panel_assignment_ids":panel_assignment_ids}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &body_hash,
        Some(evaluation_id),
        StatusCode::CREATED,
        &evaluation,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(evaluation)))
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
        let _finality_seal_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_seal_guard,
            paper_id,
        )?;
        let mut next = memory.clone();
        let context = review_context_memory(&next, paper_id)?;
        let submission = load_submission_memory(&next, paper_id, request.submission_id)?;
        let ledger =
            load_contribution_ledger_memory(&next, paper_id, &request.release_candidate_hash)?;
        let version = validate_supersession_memory(&next, paper_id, &request)?;
        ensure_evaluation_draft_slot_memory(
            &next,
            paper_id,
            submission.submission_id,
            version,
            request.evaluation_id,
        )?;
        let prepared =
            validate_evaluation_context(&context, &submission, &ledger, &request, now, true)?;
        let assignments = next
            .review
            .assignments
            .values()
            .cloned()
            .collect::<Vec<_>>();
        enforce_evaluation_assignments(
            paper_id,
            submission.submission_id,
            version,
            &request,
            assignments.iter(),
            now,
        )?;
        let panel_assignment_ids = claimed_evaluation_panel_assignment_ids(
            paper_id,
            submission.submission_id,
            version,
            &request,
            &assignments,
            now,
        )?;
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
        for assignment_id in &panel_assignment_ids {
            transition_review_assignment_memory(
                &mut next.review,
                *assignment_id,
                ReviewAssignmentStatus::Pinned,
                Some(evaluation.evaluation_id),
                now,
            )?;
            transition_review_assignment_memory(
                &mut next.review,
                *assignment_id,
                ReviewAssignmentStatus::Consumed,
                None,
                now,
            )?;
        }
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
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let context = review_context_postgres(&mut tx, paper_id).await?;
    let submission = load_submission_postgres(&mut tx, paper_id, request.submission_id).await?;
    let ledger =
        load_contribution_ledger_postgres(&mut tx, paper_id, &request.release_candidate_hash)
            .await?;
    let candidate_version = validate_supersession_postgres(&mut tx, paper_id, &request).await?;
    lock_evaluation_round_postgres(&mut tx, paper_id, candidate_version).await?;
    lock_evaluation_identity_postgres(&mut tx, request.evaluation_id).await?;
    let version = validate_supersession_postgres(&mut tx, paper_id, &request).await?;
    if version != candidate_version {
        return Err(ApiError::conflict(
            "evaluation_round_changed",
            "review round changed while the legacy evaluation was being created",
        ));
    }
    ensure_evaluation_draft_slot_postgres(
        &mut tx,
        paper_id,
        submission.submission_id,
        version,
        request.evaluation_id,
    )
    .await?;
    let prepared =
        validate_evaluation_context(&context, &submission, &ledger, &request, now, true)?;
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    enforce_evaluation_assignments(
        paper_id,
        submission.submission_id,
        version,
        &request,
        assignments.iter(),
        now,
    )?;
    let panel_assignment_ids = claimed_evaluation_panel_assignment_ids(
        paper_id,
        submission.submission_id,
        version,
        &request,
        &assignments,
        now,
    )?;
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
    for assignment_id in &panel_assignment_ids {
        transition_review_assignment_postgres(
            &mut tx,
            *assignment_id,
            ReviewAssignmentStatus::Pinned,
            Some(evaluation.evaluation_id),
            now,
        )
        .await?;
        transition_review_assignment_postgres(
            &mut tx,
            *assignment_id,
            ReviewAssignmentStatus::Consumed,
            None,
            now,
        )
        .await?;
    }
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
        let _finality_seal_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_seal_guard,
            paper_id,
        )?;
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
        ensure_evaluation_activated(&evaluation, &evaluations, &appeals, &resolutions)?;
        let version = reproduction_version_memory(&next, evaluation_id, &request)?;
        let (report, frame) = prepare_reproduction(&context, &evaluation, &request, version, now)?;
        enforce_reproducer_assignment(
            paper_id,
            evaluation.submission_id,
            evaluation.version,
            request.reproducer_player_id,
            next.review.assignments.values(),
            now,
        )?;
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
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
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
    let evaluations: Vec<PaperEvaluation> = load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let appeals: Vec<PaperAppeal> =
        load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal").await?;
    let resolutions: Vec<PaperAppealResolution> = load_review_records(
        &mut tx,
        "hepta_paper_appeal_resolutions",
        paper_id,
        "Appeal resolution",
    )
    .await?;
    ensure_evaluation_activated(&evaluation, &evaluations, &appeals, &resolutions)?;
    let version = reproduction_version_postgres(&mut tx, evaluation_id, &request).await?;
    let (report, frame) = prepare_reproduction(&context, &evaluation, &request, version, now)?;
    let assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    enforce_reproducer_assignment(
        paper_id,
        evaluation.submission_id,
        evaluation.version,
        request.reproducer_player_id,
        assignments.iter(),
        now,
    )?;
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
        let _finality_seal_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_seal_guard,
            paper_id,
        )?;
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
        ensure_evaluation_activated(&evaluation, &evaluations, &appeals, &resolutions)?;
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
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
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
    let evaluations: Vec<PaperEvaluation> = load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let appeals: Vec<PaperAppeal> =
        load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal").await?;
    let resolutions: Vec<PaperAppealResolution> = load_review_records(
        &mut tx,
        "hepta_paper_appeal_resolutions",
        paper_id,
        "Appeal resolution",
    )
    .await?;
    ensure_evaluation_activated(&evaluation, &evaluations, &appeals, &resolutions)?;
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

fn prepared_replacement_for_resolution<'a>(
    evaluation: &PaperEvaluation,
    evaluations: impl Iterator<Item = &'a PaperEvaluation>,
) -> Result<Option<&'a PaperEvaluation>, ApiError> {
    let mut prepared = evaluations.filter(|candidate| {
        candidate.paper_project_id == evaluation.paper_project_id
            && candidate.submission_id == evaluation.submission_id
            && candidate.supersedes_evaluation_id == Some(evaluation.evaluation_id)
    });
    let replacement = prepared.next();
    if prepared.next().is_some() {
        return Err(ApiError::conflict(
            "superseding_evaluation_ambiguous",
            "Appeal resolution requires at most one exact prepared replacement evaluation",
        ));
    }
    Ok(replacement)
}

fn validate_resolution_replacement(
    evaluation: &PaperEvaluation,
    superseding: Option<&PaperEvaluation>,
    outcome: AppealOutcome,
    requested_superseding_evaluation_id: Option<Uuid>,
) -> Result<(), ApiError> {
    match outcome {
        AppealOutcome::Upheld => {
            let replacement = superseding.ok_or_else(|| {
                ApiError::conflict(
                    "superseding_evaluation_required",
                    "upheld Appeal requires the immutable superseding evaluation",
                )
            })?;
            if requested_superseding_evaluation_id != Some(replacement.evaluation_id)
                || replacement.supersedes_evaluation_id != Some(evaluation.evaluation_id)
                || replacement.paper_project_id != evaluation.paper_project_id
                || replacement.submission_id != evaluation.submission_id
                || replacement.release_candidate_hash != evaluation.release_candidate_hash
                || replacement.paper_bundle_hash != evaluation.paper_bundle_hash
            {
                return Err(ApiError::conflict(
                    "invalid_superseding_evaluation",
                    "upheld Appeal must reference the exact same-release prepared replacement evaluation",
                ));
            }
        }
        AppealOutcome::Denied => {
            if requested_superseding_evaluation_id.is_some() || superseding.is_some() {
                return Err(ApiError::conflict(
                    "denied_appeal_cannot_supersede",
                    "denied Appeal must not omit or reference an already prepared replacement evaluation",
                ));
            }
        }
    }
    Ok(())
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
    validate_resolution_replacement(
        evaluation,
        superseding,
        request.outcome.clone(),
        request.superseding_evaluation_id,
    )?;
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
        let _finality_seal_guard = state.paper_chain_finality.read().await;
        crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
            &_finality_seal_guard,
            paper_id,
        )?;
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
        let superseding =
            prepared_replacement_for_resolution(&evaluation, next.review.evaluations.values())?;
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
        ensure_appealed_evaluation_activated_for_resolution(
            &evaluation,
            superseding,
            &evaluations,
            &appeals,
            &resolutions,
        )?;
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
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
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
    let evaluations: Vec<PaperEvaluation> = load_review_records(
        &mut tx,
        "hepta_paper_evaluations",
        paper_id,
        "paper evaluation",
    )
    .await?;
    let appeals: Vec<PaperAppeal> =
        load_review_records(&mut tx, "hepta_paper_appeals", paper_id, "paper Appeal").await?;
    let resolutions: Vec<PaperAppealResolution> = load_review_records(
        &mut tx,
        "hepta_paper_appeal_resolutions",
        paper_id,
        "Appeal resolution",
    )
    .await?;
    let superseding_rows = sqlx::query(
        "select record_json from hepta_paper_evaluations
         where paper_project_id=$1 and supersedes_evaluation_id=$2
         order by evaluation_id for share",
    )
    .bind(paper_id)
    .bind(evaluation.evaluation_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let superseding_records = superseding_rows
        .iter()
        .map(|row| {
            decode_record::<PaperEvaluation>(row.get("record_json"), "superseding evaluation")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let superseding = prepared_replacement_for_resolution(&evaluation, superseding_records.iter())?;
    ensure_appealed_evaluation_activated_for_resolution(
        &evaluation,
        superseding,
        &evaluations,
        &appeals,
        &resolutions,
    )?;
    let (resolution, frame) =
        prepare_resolution(&context, &appeal, &evaluation, superseding, &request, now)?;
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
    team: &ResearchTeam,
    model: &PaperReviewReadModel,
    now: DateTime<Utc>,
) -> bool {
    team.members
        .iter()
        .any(|member| member.player_id == player_id)
        || model.assignments.iter().any(|assignment| {
            assignment.player_id == player_id && review_assignment_active(assignment, now)
        })
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

fn review_state_evaluation_drafts<'a>(
    records: impl Iterator<Item = &'a PaperEvaluationDraft>,
    paper_id: Uuid,
) -> Vec<PaperEvaluationDraft> {
    records
        .filter(|record| {
            record.paper_project_id == paper_id && record.status != EvaluationDraftStatus::Finalized
        })
        .cloned()
        .collect()
}

async fn get_paper_review_state(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PaperReviewReadModel>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/review-state");
    let assertion =
        require_member_read_assertion(&headers, &state, "get_paper_review_state_v1", &path)?;
    let now = Utc::now();
    if state.pool.is_none() {
        let finality = state
            .paper_chain_finality
            .read()
            .await
            .projection_for_paper(paper_id)
            .map(ConsumerPaperFinalityV2::from_projection)
            .unwrap_or_else(ConsumerPaperFinalityV2::pending);
        let memory = state.paper_raid.read().await;
        active_registered_player_memory(&memory, &assertion)?;
        let context = review_access_context_memory(&memory, paper_id)?;
        let mut assignments = memory
            .review
            .assignments
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut projected_drafts = memory
            .review
            .evaluation_drafts
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .cloned()
            .collect::<Vec<_>>();
        project_evaluation_draft_expirations(&mut projected_drafts, &mut assignments, now)?;
        let evaluation_drafts = review_state_evaluation_drafts(projected_drafts.iter(), paper_id);
        let mut model = PaperReviewReadModel {
            finality,
            assignments,
            evaluation_drafts,
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
        if !review_actor_allowed(assertion.player_id, &context.team, &model, now) {
            return Err(ApiError::forbidden(
                "review_state_access_denied",
                "review state is limited to authors, assigned actors, and signed review participants",
            ));
        }
        project_settlement_states(&mut model.evaluations, &model.appeals, &model.resolutions);
        sort_review_model(&mut model);
        return Ok(Json(model));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    active_registered_player_postgres(&mut tx, &assertion).await?;
    let context = review_access_context_postgres(&mut tx, paper_id).await?;
    let finality = sqlx::query(
        "select record_json from hepta_paper_chain_finality_projections
         where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .map(|row| {
        decode_record::<PaperChainFinalityProjectionV2>(
            row.get("record_json"),
            "Paper Chain finality projection",
        )
        .map(ConsumerPaperFinalityV2::from_projection)
    })
    .transpose()?
    .unwrap_or_else(ConsumerPaperFinalityV2::pending);
    let mut projected_drafts = load_review_records::<PaperEvaluationDraft>(
        &mut tx,
        "hepta_paper_evaluation_drafts",
        paper_id,
        "paper evaluation draft",
    )
    .await?;
    let mut assignments = load_review_assignments_postgres(&mut tx, paper_id).await?;
    project_evaluation_draft_expirations(&mut projected_drafts, &mut assignments, now)?;
    let evaluation_drafts = review_state_evaluation_drafts(projected_drafts.iter(), paper_id);
    let mut model = PaperReviewReadModel {
        finality,
        assignments,
        evaluation_drafts,
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
    if !review_actor_allowed(assertion.player_id, &context.team, &model, now) {
        return Err(ApiError::forbidden(
            "review_state_access_denied",
            "review state is limited to authors, assigned actors, and signed review participants",
        ));
    }
    project_settlement_states(&mut model.evaluations, &model.appeals, &model.resolutions);
    sort_review_model(&mut model);
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(model))
}

pub(super) async fn load_review_records<T: serde::de::DeserializeOwned>(
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
    model.assignments.sort_by_key(|record| {
        (
            record.review_round,
            record.slot.rank(),
            record.assignment_id,
        )
    });
    model
        .evaluation_drafts
        .sort_by_key(|record| record.evaluation_id);
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
    fn migration_0051_is_atomic_and_preserves_both_panel_authority_paths() {
        let migration =
            include_str!("../../../migrations/0051_bind_legacy_evaluation_panel_lifecycle.sql");
        assert!(migration.contains("\nbegin;\n"));
        assert!(migration.trim_end().ends_with("commit;"));
        assert!(migration.contains("public.hepta_paper_evaluation_drafts"));
        assert!(migration.contains("public.hepta_paper_evaluations"));
        assert!(migration.contains("public.hepta_paper_evaluation_panel_attestations"));
        assert!(migration
            .contains("enable always trigger hepta_review_assignment_draft_lifecycle_guard"));
        assert!(!migration.contains(" or true"));
    }

    #[test]
    fn review_object_keys_are_deterministic_and_paper_scoped() {
        let object = NeutralArtifactObjectV1 {
            canonical_json: false,
            dependencies: Vec::new(),
            logical_path: "inputs/dataset.json".to_string(),
            media_type: "application/json".to_string(),
            role: "dataset".to_string(),
            sha256: "a".repeat(64),
            size: 32,
        };
        let manifest_hash = format!("sha256:{}", "b".repeat(64));
        let first = paper_scoped_review_object_key(Uuid::from_u128(1), &manifest_hash, &object)
            .expect("first Paper-scoped key");
        let same = paper_scoped_review_object_key(Uuid::from_u128(1), &manifest_hash, &object)
            .expect("deterministic Paper-scoped key");
        let foreign = paper_scoped_review_object_key(Uuid::from_u128(2), &manifest_hash, &object)
            .expect("foreign Paper-scoped key");
        assert_eq!(first, same);
        assert_ne!(first, foreign);
        assert!(first.starts_with("review-object-") && first.len() == 78);
    }

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

    #[test]
    fn contribution_xp_is_capped_by_milestone_not_record_count() {
        assert_eq!(milestone_contribution_points(0, 0), 0);
        assert_eq!(milestone_contribution_points(1, 0), 100);
        assert_eq!(milestone_contribution_points(257, 0), 100);
        assert_eq!(milestone_contribution_points(0, 1), 150);
        assert_eq!(milestone_contribution_points(0, 257), 150);
        assert_eq!(milestone_contribution_points(257, 257), 250);
    }

    fn contribution_author(player_id: Uuid, order: u32) -> PaperReleaseAuthorV2 {
        PaperReleaseAuthorV2 {
            author_order: order,
            participant_slot: order,
            player_id,
            display_name: format!("Author {order}"),
            credit_roles: vec!["methodology".to_string()],
        }
    }

    fn contribution_manifest(paper_id: Uuid, manifest_id: Uuid) -> ArtifactManifest {
        ArtifactManifest {
            manifest_id,
            paper_project_id: paper_id,
            binding_schema: "fixture".to_string(),
            source_bundle_schema: "fixture".to_string(),
            source_bundle_id: "fixture".to_string(),
            source_challenge_id: Uuid::new_v4().to_string(),
            source_created_at: "2026-08-11T00:00:00Z".to_string(),
            source_manifest_sha256: "0".repeat(64),
            manifest_hash: format!("sha256:{}", "0".repeat(64)),
            object_count: 0,
            objects: Vec::new(),
            required_run_ids: Vec::new(),
            storage_locations: Vec::new(),
            total_size_bytes: 0,
            review_ready_assembly: None,
            version: 1,
            created_at: Utc::now(),
        }
    }

    fn accepted_contribution_proposal(
        paper_id: Uuid,
        proposal_id: Uuid,
        manifest_id: Uuid,
    ) -> AgentProposal {
        let digest = format!("sha256:{}", "1".repeat(64));
        AgentProposal {
            proposal_id,
            paper_project_id: paper_id,
            work_item_id: Uuid::new_v4(),
            section_key: "methods".to_string(),
            parent_revision_id: Uuid::new_v4(),
            lease_id: Uuid::new_v4(),
            lease_fencing_token: 1,
            expected_work_version: 1,
            proposal_kind: AgentProposalKind::Delivery,
            payload_hash: digest.clone(),
            artifact_manifest_id: manifest_id,
            artifact_manifest_hash: digest.clone(),
            agent_id: format!("agent-{proposal_id}"),
            binding_id: Uuid::new_v4(),
            agent_key_id: digest,
            agent_public_key: "fixture".to_string(),
            signed_at_unix: 1,
            signature: "fixture".to_string(),
            status: AgentProposalStatus::Accepted,
            version: 2,
            updated_at: Utc::now(),
        }
    }

    fn contribution_acceptance(
        paper_id: Uuid,
        proposal_id: Uuid,
        player_id: Uuid,
    ) -> HumanDecision {
        let digest = format!("sha256:{}", "2".repeat(64));
        HumanDecision {
            decision_id: Uuid::new_v4(),
            paper_project_id: paper_id,
            proposal_id,
            player_id,
            decision: HumanDecisionKind::Accept,
            reason_hash: digest.clone(),
            signing_key_id: "fixture".to_string(),
            signing_public_key: "fixture".to_string(),
            signing_public_key_hash: digest,
            signed_at_unix: 1,
            signature: "fixture".to_string(),
            version: 1,
        }
    }

    fn approved_contribution_review(
        paper_id: Uuid,
        review_id: Uuid,
        player_id: Uuid,
    ) -> SectionReview {
        let digest = format!("sha256:{}", "3".repeat(64));
        SectionReview {
            review_id,
            paper_project_id: paper_id,
            section_revision_id: Uuid::new_v4(),
            reviewer_player_id: player_id,
            verdict: SectionReviewVerdict::Approve,
            review_hash: digest.clone(),
            signing_key_id: "fixture".to_string(),
            signing_public_key: "fixture".to_string(),
            signing_public_key_hash: digest,
            signed_at_unix: 1,
            signature: "fixture".to_string(),
            version: 1,
        }
    }

    #[test]
    fn authoritative_257_refs_are_preserved_with_milestone_points() {
        let paper_id = Uuid::new_v4();
        let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let authors = players
            .iter()
            .enumerate()
            .map(|(index, player_id)| contribution_author(*player_id, index as u32 + 1))
            .collect::<Vec<_>>();
        let reviews = (1..=257_u128)
            .map(|value| approved_contribution_review(paper_id, Uuid::from_u128(value), players[0]))
            .collect::<Vec<_>>();
        let entries = authoritative_contribution_entries_from_records(
            paper_id,
            &authors,
            &[],
            &[],
            &[],
            &reviews,
        )
        .expect("257 scientific references must remain freezeable");
        let credited = entries
            .iter()
            .find(|entry| entry.player_id == players[0])
            .expect("credited author");
        assert_eq!(credited.accepted_section_review_ids.len(), 257);
        assert_eq!(credited.contribution_points, 150);
    }

    #[test]
    fn duplicate_and_cross_author_manifest_credit_is_rejected() {
        let duplicate = Uuid::new_v4();
        let error = normalize_uuid_set("accepted_artifact_manifest_ids", &[duplicate, duplicate])
            .expect_err("one entry cannot repeat a reference");
        assert_eq!(error.code, "duplicate_contribution_reference");

        let paper_id = Uuid::new_v4();
        let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let authors = players
            .iter()
            .enumerate()
            .map(|(index, player_id)| contribution_author(*player_id, index as u32 + 1))
            .collect::<Vec<_>>();
        let manifest_id = Uuid::new_v4();
        let proposals = [
            accepted_contribution_proposal(paper_id, Uuid::new_v4(), manifest_id),
            accepted_contribution_proposal(paper_id, Uuid::new_v4(), manifest_id),
        ];
        let decisions = [
            contribution_acceptance(paper_id, proposals[0].proposal_id, players[0]),
            contribution_acceptance(paper_id, proposals[1].proposal_id, players[1]),
        ];
        let error = authoritative_contribution_entries_from_records(
            paper_id,
            &authors,
            &[contribution_manifest(paper_id, manifest_id)],
            &proposals,
            &decisions,
            &[],
        )
        .expect_err("one manifest cannot be credited to two authors");
        assert_eq!(error.code, "artifact_contribution_duplicated");
    }

    #[test]
    fn added_or_cross_author_client_refs_are_rejected() {
        let players = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let shared_manifest = Uuid::new_v4();
        let submitted = vec![
            CreditContribution {
                player_id: players[0],
                credit_roles: vec!["methodology".to_string()],
                accepted_artifact_manifest_ids: vec![shared_manifest],
                accepted_section_review_ids: Vec::new(),
                contribution_points: 100,
            },
            CreditContribution {
                player_id: players[1],
                credit_roles: vec!["methodology".to_string()],
                accepted_artifact_manifest_ids: vec![shared_manifest],
                accepted_section_review_ids: Vec::new(),
                contribution_points: 100,
            },
            CreditContribution {
                player_id: players[2],
                credit_roles: vec!["methodology".to_string()],
                accepted_artifact_manifest_ids: Vec::new(),
                accepted_section_review_ids: Vec::new(),
                contribution_points: 0,
            },
        ];
        assert_eq!(
            duplicate_contribution_reference_kind(&submitted),
            Some("artifact manifest")
        );

        let authoritative = submitted
            .iter()
            .cloned()
            .map(|mut entry| {
                entry.accepted_artifact_manifest_ids.clear();
                entry.contribution_points = 0;
                entry
            })
            .collect::<Vec<_>>();
        let error =
            require_complete_authoritative_contribution_entries(&submitted[1..], &authoritative)
                .expect_err("added or incomplete client facts must not freeze");
        assert_eq!(error.code, "contribution_ledger_entries_mismatch");
    }

    #[test]
    fn ledger_id_reservation_rejects_nil_and_global_preemption() {
        assert_eq!(
            validate_contribution_ledger_id(Uuid::nil())
                .expect_err("nil ledger IDs are forbidden")
                .code,
            "nil_contribution_ledger_id"
        );
        let mut memory = PaperRaidMemory::default();
        let ledger_id = Uuid::new_v4();
        let first_paper = Uuid::new_v4();
        let second_paper = Uuid::new_v4();
        let release_hash = format!("sha256:{}", "4".repeat(64));
        reserve_contribution_ledger_id_memory(&mut memory, ledger_id, first_paper, &release_hash)
            .expect("first promotion owns ID");
        let error = reserve_contribution_ledger_id_memory(
            &mut memory,
            ledger_id,
            second_paper,
            &release_hash,
        )
        .expect_err("another Paper cannot preempt the ID");
        assert_eq!(error.code, "contribution_ledger_id_reserved");
    }

    #[test]
    fn ledger_loader_requires_the_exact_memory_reservation_triple() {
        let paper_id = Uuid::new_v4();
        let ledger_id = Uuid::new_v4();
        let release_hash = format!("sha256:{}", "6".repeat(64));
        let created_at = Utc::now();
        let mut entries = (1..=3_u128)
            .map(|value| CreditContribution {
                player_id: Uuid::from_u128(value),
                credit_roles: vec!["methodology".to_string()],
                accepted_artifact_manifest_ids: Vec::new(),
                accepted_section_review_ids: Vec::new(),
                contribution_points: 0,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.player_id);
        let ledger = ContributionLedger {
            schema: CONTRIBUTION_LEDGER_SCHEMA_V1.to_string(),
            contribution_ledger_id: ledger_id,
            paper_project_id: paper_id,
            release_candidate_hash: release_hash.clone(),
            ledger_hash: contribution_ledger_hash(ledger_id, paper_id, &entries).unwrap(),
            entries,
            version: 1,
            created_at,
        };
        let mut memory = PaperRaidMemory::default();
        memory
            .review
            .contribution_ledgers
            .insert(ledger_id, ledger.clone());

        let missing = load_contribution_ledger_memory(&memory, paper_id, &release_hash)
            .expect_err("an unreserved ledger must never reach RaidScore");
        assert_eq!(missing.code, "contribution_ledger_id_not_reserved");

        reserve_contribution_ledger_id_memory(&mut memory, ledger_id, paper_id, &release_hash)
            .expect("exact reservation");
        reserve_contribution_ledger_id_memory(&mut memory, ledger_id, paper_id, &release_hash)
            .expect("exact reservation replay is idempotent");
        assert_eq!(
            load_contribution_ledger_memory(&memory, paper_id, &release_hash).unwrap(),
            ledger
        );

        memory
            .contribution_ledger_reservations
            .get_mut(&ledger_id)
            .expect("reservation")
            .release_candidate_hash = format!("sha256:{}", "7".repeat(64));
        let mismatch = load_contribution_ledger_memory(&memory, paper_id, &release_hash)
            .expect_err("a mismatched reservation must never reach RaidScore");
        assert_eq!(mismatch.code, "contribution_ledger_id_ownership_mismatch");
    }

    #[test]
    fn canonical_loader_rejects_record_json_entry_drift_before_raid_score() {
        let paper_id = Uuid::new_v4();
        let ledger_id = Uuid::new_v4();
        let release_hash = format!("sha256:{}", "5".repeat(64));
        let mut entries = (1..=3_u128)
            .map(|value| CreditContribution {
                player_id: Uuid::from_u128(value),
                credit_roles: vec!["methodology".to_string()],
                accepted_artifact_manifest_ids: Vec::new(),
                accepted_section_review_ids: Vec::new(),
                contribution_points: 0,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.player_id);
        let ledger_hash = contribution_ledger_hash(ledger_id, paper_id, &entries).unwrap();
        let created_at = Utc::now();
        let relational_entries = serde_json::to_value(&entries).unwrap();
        let mut drifted = ContributionLedger {
            schema: CONTRIBUTION_LEDGER_SCHEMA_V1.to_string(),
            contribution_ledger_id: ledger_id,
            paper_project_id: paper_id,
            release_candidate_hash: release_hash.clone(),
            entries,
            ledger_hash: ledger_hash.clone(),
            version: 1,
            created_at,
        };
        drifted.entries[0]
            .accepted_artifact_manifest_ids
            .push(Uuid::new_v4());
        drifted.entries[0].contribution_points = 100;
        let error = validate_loaded_contribution_ledger(
            drifted,
            ledger_id,
            paper_id,
            &release_hash,
            &ledger_hash,
            &relational_entries,
            1,
            created_at,
        )
        .expect_err("record_json-only entry drift must fail closed");
        assert_eq!(error.code, "internal_error");
    }

    fn assignment_fixture(
        paper_project_id: Uuid,
        player_id: Uuid,
        slot: ReviewAssignmentSlot,
    ) -> ReviewAssignment {
        let now = Utc::now();
        ReviewAssignment {
            schema: REVIEW_ASSIGNMENT_SCHEMA_V1.to_string(),
            assignment_id: Uuid::new_v4(),
            paper_project_id,
            submission_id: Uuid::new_v4(),
            player_id,
            review_round: 1,
            slot,
            pinned_evaluation_id: None,
            status: ReviewAssignmentStatus::Claimed,
            version: 1,
            claimed_at: now,
            expires_at: now + chrono::Duration::hours(1),
            updated_at: now,
        }
    }

    fn evaluation_draft_fixture(
        paper_project_id: Uuid,
        submission_id: Uuid,
        evaluator_player_id: Uuid,
        now: DateTime<Utc>,
    ) -> PaperEvaluationDraft {
        let evaluation_id = Uuid::new_v4();
        let digest = format!("sha256:{}", "0".repeat(64));
        let mut draft = PaperEvaluationDraft {
            schema: EVALUATION_DRAFT_SCHEMA_V2.to_string(),
            evaluation_id,
            paper_project_id,
            submission_id,
            review_round: 1,
            supersedes_evaluation_id: None,
            release_candidate_hash: digest.clone(),
            paper_bundle_hash: digest.clone(),
            tolerance_policy: TolerancePolicy {
                schema: TOLERANCE_POLICY_SCHEMA_V1.to_string(),
                version: "fixture-v1".to_string(),
                rules: Vec::new(),
            },
            tolerance_policy_hash: digest.clone(),
            reference_metrics_micros: BTreeMap::new(),
            paper_score: PaperScore {
                schema: PAPER_SCORE_SCHEMA_V1.to_string(),
                evaluation_id,
                paper_project_id,
                components: PaperScoreComponents {
                    method_rigor_bps: 0,
                    experiment_statistics_bps: 0,
                    reproducibility_bps: 0,
                    evidence_citations_bps: 0,
                    value_originality_bps: 0,
                    argument_expression_bps: 0,
                    ethics_transparency_bps: 0,
                },
                hard_gates: PaperHardGates {
                    citations_and_data_authentic: false,
                    failed_runs_disclosed: false,
                    all_authors_consented: false,
                    core_claims_have_evidence: false,
                    artifact_lineage_complete: false,
                    license_ethics_coi_complete: false,
                },
                score_bps: 0,
                eligible: false,
                score_hash: digest.clone(),
                created_at: now,
            },
            evaluator_player_id,
            evaluator_signing_key_id: "fixture-key".to_string(),
            evaluator_signing_public_key: "fixture-public-key".to_string(),
            evaluator_signing_public_key_hash: digest.clone(),
            evaluator_coi_attestation_hash: digest.clone(),
            evaluator_signed_at_unix: now.timestamp(),
            evaluator_signature: "fixture-signature".to_string(),
            evaluation_signing_hash: digest,
            draft_hash: String::new(),
            status: EvaluationDraftStatus::Open,
            version: 1,
            finalized_evaluation_id: None,
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
            updated_at: now,
            finalized_at: None,
            expired_at: None,
        };
        draft.draft_hash = evaluation_draft_hash(&draft).expect("fixture draft hash");
        draft
    }

    fn evaluation_fixture_from_draft(draft: &PaperEvaluationDraft) -> PaperEvaluation {
        PaperEvaluation {
            schema: PAPER_EVALUATION_V1.to_string(),
            evaluation_id: draft.evaluation_id,
            paper_project_id: draft.paper_project_id,
            submission_id: draft.submission_id,
            release_candidate_hash: draft.release_candidate_hash.clone(),
            paper_bundle_hash: draft.paper_bundle_hash.clone(),
            supersedes_evaluation_id: draft.supersedes_evaluation_id,
            tolerance_policy: draft.tolerance_policy.clone(),
            tolerance_policy_hash: draft.tolerance_policy_hash.clone(),
            reference_metrics_micros: draft.reference_metrics_micros.clone(),
            evaluator_player_id: draft.evaluator_player_id,
            evaluator_signing_key_id: draft.evaluator_signing_key_id.clone(),
            evaluator_signing_public_key: draft.evaluator_signing_public_key.clone(),
            evaluator_signing_public_key_hash: draft.evaluator_signing_public_key_hash.clone(),
            evaluator_coi_attestation_hash: draft.evaluator_coi_attestation_hash.clone(),
            evaluator_signed_at_unix: draft.evaluator_signed_at_unix,
            evaluator_signature: draft.evaluator_signature.clone(),
            reviewer_attestations: Vec::new(),
            paper_score: draft.paper_score.clone(),
            status: PaperEvaluationStatus::NotEligible,
            settlement_state: ReviewSettlementState::PendingFinality,
            evaluation_signing_hash: draft.evaluation_signing_hash.clone(),
            version: draft.review_round,
            created_at: draft.created_at,
        }
    }

    fn review_attestation_fixture(
        evaluation: &PaperEvaluation,
        reviewer_player_id: Uuid,
        marker: char,
    ) -> ReviewAttestation {
        let digest = format!("sha256:{}", marker.to_string().repeat(64));
        ReviewAttestation {
            attestation_id: Uuid::new_v4(),
            evaluation_id: evaluation.evaluation_id,
            evaluation_signing_hash: evaluation.evaluation_signing_hash.clone(),
            reviewer_player_id,
            verdict: PanelVerdict::Approve,
            signing_key_id: format!("fixture-reviewer-{marker}-key"),
            signing_public_key: format!("fixture-reviewer-{marker}-public-key"),
            signing_public_key_hash: digest.clone(),
            coi_attestation_hash: digest,
            signed_at_unix: evaluation.created_at.timestamp(),
            signature: format!("fixture-reviewer-{marker}-signature"),
        }
    }

    #[test]
    fn review_state_exposes_only_same_paper_unresolved_draft_authority() {
        let paper_id = Uuid::new_v4();
        let now = Utc::now();
        let open = evaluation_draft_fixture(paper_id, Uuid::new_v4(), Uuid::new_v4(), now);
        let open_id = open.evaluation_id;
        let mut expired = evaluation_draft_fixture(paper_id, Uuid::new_v4(), Uuid::new_v4(), now);
        expired.status = EvaluationDraftStatus::Expired;
        let expired_id = expired.evaluation_id;
        let mut finalized = evaluation_draft_fixture(paper_id, Uuid::new_v4(), Uuid::new_v4(), now);
        finalized.status = EvaluationDraftStatus::Finalized;
        let other_paper =
            evaluation_draft_fixture(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), now);
        // Selection is independent of lifecycle mutation details; strict
        // consumers validate each selected record's complete lifecycle.
        let mut selected = review_state_evaluation_drafts(
            [&open, &expired, &finalized, &other_paper].into_iter(),
            paper_id,
        );
        selected.sort_by_key(|draft| draft.evaluation_id);
        let mut expected = vec![open_id, expired_id];
        expected.sort_unstable();
        assert_eq!(
            selected
                .into_iter()
                .map(|draft| draft.evaluation_id)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn review_state_projects_draft_expiry_before_treating_pinned_actor_as_active() {
        let paper_id = Uuid::new_v4();
        let evaluator_id = Uuid::new_v4();
        let now = Utc::now();
        let created_at = now - chrono::Duration::hours(2);
        let draft = evaluation_draft_fixture(paper_id, Uuid::new_v4(), evaluator_id, created_at);
        let mut assignment =
            assignment_fixture(paper_id, evaluator_id, ReviewAssignmentSlot::Evaluator);
        assignment.submission_id = draft.submission_id;
        assignment.review_round = draft.review_round;
        assignment.pinned_evaluation_id = Some(draft.evaluation_id);
        assignment.status = ReviewAssignmentStatus::Pinned;
        assignment.version = 2;
        assignment.claimed_at = created_at - chrono::Duration::minutes(1);
        assignment.expires_at = created_at + chrono::Duration::minutes(5);
        assignment.updated_at = created_at;

        let mut drafts = vec![draft];
        let mut assignments = vec![assignment];
        project_evaluation_draft_expirations(&mut drafts, &mut assignments, now)
            .expect("read projection expires stale draft authority");

        assert_eq!(drafts[0].status, EvaluationDraftStatus::Expired);
        assert_eq!(assignments[0].status, ReviewAssignmentStatus::Expired);
        assert!(!review_assignment_active(&assignments[0], now));
        assert_eq!(assignments[0].updated_at, drafts[0].expires_at);
    }

    #[test]
    fn review_assignment_slots_have_stable_contract_names_and_vacancy_order() {
        assert_eq!(
            serde_json::to_value(ReviewAssignmentSlot::Reviewer1).unwrap(),
            json!("reviewer_1")
        );
        assert_eq!(
            serde_json::to_value(ReviewAssignmentSlot::Reviewer2).unwrap(),
            json!("reviewer_2")
        );
        assert_eq!(
            serde_json::to_value(ReviewAssignmentStatus::Pinned).unwrap(),
            json!("pinned")
        );
        assert_eq!(
            serde_json::to_value(ReviewAssignmentStatus::Consumed).unwrap(),
            json!("consumed")
        );
        let paper_id = Uuid::new_v4();
        let now = Utc::now();
        let assignments = [assignment_fixture(
            paper_id,
            Uuid::new_v4(),
            ReviewAssignmentSlot::Evaluator,
        )];
        assert_eq!(
            claimable_review_vacancies(
                paper_id,
                assignments[0].submission_id,
                Uuid::new_v4(),
                &[],
                &[],
                &[],
                &[],
                &assignments,
                now,
            )
            .unwrap(),
            vec![
                ReviewAssignmentVacancy {
                    review_round: 1,
                    slot: ReviewAssignmentSlot::Reviewer1,
                },
                ReviewAssignmentVacancy {
                    review_round: 1,
                    slot: ReviewAssignmentSlot::Reviewer2,
                },
            ]
        );
    }

    #[test]
    fn reproducer_vacancy_waits_for_exact_upheld_replacement_activation() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let now = Utc::now();
        let mut root = evaluation_fixture_from_draft(&evaluation_draft_fixture(
            paper_id,
            submission_id,
            Uuid::new_v4(),
            now - chrono::Duration::minutes(3),
        ));
        root.reviewer_attestations = vec![
            review_attestation_fixture(&root, Uuid::new_v4(), 'a'),
            review_attestation_fixture(&root, Uuid::new_v4(), 'b'),
        ];
        let appeal = PaperAppeal {
            schema: PAPER_APPEAL_V1.to_string(),
            appeal_id: Uuid::new_v4(),
            evaluation_id: root.evaluation_id,
            paper_project_id: paper_id,
            release_candidate_hash: root.release_candidate_hash.clone(),
            appellant_player_id: Uuid::new_v4(),
            grounds_hash: format!("sha256:{}", "1".repeat(64)),
            evidence_manifest_hash: format!("sha256:{}", "2".repeat(64)),
            signing_key_id: "fixture-appeal-key".to_string(),
            signing_public_key: "fixture-appeal-public-key".to_string(),
            signing_public_key_hash: format!("sha256:{}", "3".repeat(64)),
            signed_at_unix: now.timestamp(),
            signature: "fixture-appeal-signature".to_string(),
            version: 1,
            created_at: now - chrono::Duration::minutes(2),
        };
        let mut replacement = root.clone();
        replacement.evaluation_id = Uuid::new_v4();
        replacement.supersedes_evaluation_id = Some(root.evaluation_id);
        replacement.version = 2;
        replacement.created_at = now - chrono::Duration::minutes(1);
        replacement.evaluator_player_id = Uuid::new_v4();
        replacement.evaluation_signing_hash = format!("sha256:{}", "c".repeat(64));
        replacement.reviewer_attestations = vec![
            review_attestation_fixture(&replacement, Uuid::new_v4(), 'd'),
            review_attestation_fixture(&replacement, Uuid::new_v4(), 'e'),
        ];
        replacement.paper_score.evaluation_id = replacement.evaluation_id;
        replacement.paper_score.created_at = replacement.created_at;
        let evaluations = vec![root.clone(), replacement.clone()];
        let player_id = Uuid::new_v4();

        assert!(ensure_evaluation_activated(
            &replacement,
            &evaluations,
            std::slice::from_ref(&appeal),
            &[]
        )
        .is_err());
        let before = claimable_review_vacancies(
            paper_id,
            submission_id,
            player_id,
            &evaluations,
            &[],
            std::slice::from_ref(&appeal),
            &[],
            &[],
            now,
        )
        .expect("inactive replacement yields a closed reproducer vacancy");
        assert!(!before.iter().any(|vacancy| {
            vacancy.review_round == 2 && vacancy.slot == ReviewAssignmentSlot::Reproducer
        }));

        let resolution = PaperAppealResolution {
            schema: PAPER_APPEAL_RESOLUTION_V1.to_string(),
            resolution_id: Uuid::new_v4(),
            appeal_id: appeal.appeal_id,
            evaluation_id: root.evaluation_id,
            paper_project_id: paper_id,
            release_candidate_hash: root.release_candidate_hash.clone(),
            outcome: AppealOutcome::Upheld,
            superseding_evaluation_id: Some(replacement.evaluation_id),
            decision_hash: format!("sha256:{}", "4".repeat(64)),
            resolver_player_id: Uuid::new_v4(),
            signing_key_id: "fixture-resolution-key".to_string(),
            signing_public_key: "fixture-resolution-public-key".to_string(),
            signing_public_key_hash: format!("sha256:{}", "5".repeat(64)),
            signed_at_unix: now.timestamp(),
            signature: "fixture-resolution-signature".to_string(),
            version: 1,
            created_at: now,
        };
        ensure_evaluation_activated(
            &replacement,
            &evaluations,
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
        )
        .expect("exact upheld resolution activates replacement");
        let mut reproduction = PaperReproduction {
            schema: PAPER_REPRODUCTION_V1.to_string(),
            reproduction_id: Uuid::new_v4(),
            evaluation_id: replacement.evaluation_id,
            paper_project_id: paper_id,
            release_candidate_hash: replacement.release_candidate_hash.clone(),
            paper_bundle_hash: replacement.paper_bundle_hash.clone(),
            tolerance_policy_hash: replacement.tolerance_policy_hash.clone(),
            observed_metrics_micros: BTreeMap::new(),
            statistical_evidence: BTreeMap::new(),
            seed_set_hash: format!("sha256:{}", "6".repeat(64)),
            environment_hash: format!("sha256:{}", "7".repeat(64)),
            run_manifest_hash: format!("sha256:{}", "8".repeat(64)),
            supersedes_reproduction_id: None,
            reproducer_player_id: Uuid::new_v4(),
            signing_key_id: "fixture-reproducer-key".to_string(),
            signing_public_key: "fixture-reproducer-public-key".to_string(),
            signing_public_key_hash: format!("sha256:{}", "9".repeat(64)),
            coi_attestation_hash: format!("sha256:{}", "a".repeat(64)),
            signed_at_unix: now.timestamp(),
            signature: "fixture-reproduction-signature".to_string(),
            rule_results: Vec::new(),
            status: ReproductionStatus::Reproduced,
            report_hash: format!("sha256:{}", "b".repeat(64)),
            version: 1,
            created_at: now - chrono::Duration::seconds(30),
        };
        let mut historical_root_reproduction = reproduction.clone();
        historical_root_reproduction.evaluation_id = root.evaluation_id;
        historical_root_reproduction.created_at = root.created_at + chrono::Duration::seconds(1);
        validate_reproduction_activation(
            &replacement,
            &historical_root_reproduction,
            &evaluations,
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
        )
        .expect("a root reproduction remains valid after an exact later supersession");

        let mut equal_mutation_close_reproduction = historical_root_reproduction.clone();
        equal_mutation_close_reproduction.created_at = replacement.created_at;
        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &equal_mutation_close_reproduction,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a historical reproduction at the direct child boundary is closed")
            .code,
            "paper_finality_historical_reproduction_after_mutation_closed",
        );

        let mut late_historical_reproduction = historical_root_reproduction.clone();
        late_historical_reproduction.created_at =
            replacement.created_at + chrono::Duration::seconds(1);
        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &late_historical_reproduction,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a historical reproduction after direct child preparation is closed")
            .code,
            "paper_finality_historical_reproduction_after_mutation_closed",
        );

        let mut release_candidate_drift = historical_root_reproduction.clone();
        release_candidate_drift.release_candidate_hash = format!("sha256:{}", "1".repeat(64));
        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &release_candidate_drift,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a reproduction cannot drift from its evaluation release candidate")
            .code,
            "paper_finality_reproduction_authority_mismatch",
        );

        let mut paper_bundle_drift = historical_root_reproduction.clone();
        paper_bundle_drift.paper_bundle_hash = format!("sha256:{}", "2".repeat(64));
        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &paper_bundle_drift,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a reproduction cannot drift from its evaluation paper bundle")
            .code,
            "paper_finality_reproduction_authority_mismatch",
        );

        let mut tolerance_policy_drift = historical_root_reproduction.clone();
        tolerance_policy_drift.tolerance_policy_hash = format!("sha256:{}", "3".repeat(64));
        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &tolerance_policy_drift,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a reproduction cannot drift from its evaluation tolerance policy")
            .code,
            "paper_finality_reproduction_authority_mismatch",
        );

        assert_eq!(
            validate_reproduction_activation(
                &replacement,
                &reproduction,
                &evaluations,
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect_err("a child reproduction cannot predate its upheld activation")
            .code,
            "paper_finality_reproduction_before_activation",
        );

        let mut unrelated = evaluation_fixture_from_draft(&evaluation_draft_fixture(
            paper_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            root.created_at,
        ));
        unrelated.reviewer_attestations = vec![
            review_attestation_fixture(&unrelated, Uuid::new_v4(), '6'),
            review_attestation_fixture(&unrelated, Uuid::new_v4(), '7'),
        ];
        let mut unrelated_evaluations = evaluations.clone();
        unrelated_evaluations.push(unrelated.clone());
        let mut unrelated_reproduction = reproduction.clone();
        unrelated_reproduction.evaluation_id = unrelated.evaluation_id;
        unrelated_reproduction.created_at = now + chrono::Duration::seconds(1);
        assert!(validate_reproduction_activation(
            &replacement,
            &unrelated_reproduction,
            &unrelated_evaluations,
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
        )
        .is_err());

        let mut fork = replacement.clone();
        fork.evaluation_id = Uuid::new_v4();
        fork.evaluation_signing_hash = format!("sha256:{}", "f".repeat(64));
        fork.evaluator_player_id = Uuid::new_v4();
        fork.reviewer_attestations = vec![
            review_attestation_fixture(&fork, Uuid::new_v4(), '8'),
            review_attestation_fixture(&fork, Uuid::new_v4(), '9'),
        ];
        fork.paper_score.evaluation_id = fork.evaluation_id;
        let mut forked_evaluations = evaluations.clone();
        forked_evaluations.push(fork.clone());
        let mut fork_reproduction = reproduction.clone();
        fork_reproduction.evaluation_id = fork.evaluation_id;
        fork_reproduction.created_at = now + chrono::Duration::seconds(1);
        assert!(validate_reproduction_activation(
            &replacement,
            &fork_reproduction,
            &forked_evaluations,
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
        )
        .is_err());

        assert!(effective_finality_resolution_id(
            &replacement,
            &evaluations,
            std::slice::from_ref(&reproduction),
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
        )
        .is_err());
        reproduction.created_at = now + chrono::Duration::seconds(1);
        assert_eq!(
            effective_finality_resolution_id(
                &replacement,
                &evaluations,
                std::slice::from_ref(&reproduction),
                std::slice::from_ref(&appeal),
                std::slice::from_ref(&resolution),
            )
            .expect("post-activation reproduction supports exact Consumer finality"),
            Some(resolution.resolution_id),
        );
        let after = claimable_review_vacancies(
            paper_id,
            submission_id,
            player_id,
            &evaluations,
            &[],
            std::slice::from_ref(&appeal),
            std::slice::from_ref(&resolution),
            &[],
            now,
        )
        .expect("activated replacement exposes reproducer vacancy");
        assert!(after.iter().any(|vacancy| {
            vacancy.review_round == 2 && vacancy.slot == ReviewAssignmentSlot::Reproducer
        }));
    }

    #[test]
    fn resolution_activation_ignores_only_one_exact_direct_prepared_child() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let now = Utc::now();
        let mut root = evaluation_fixture_from_draft(&evaluation_draft_fixture(
            paper_id,
            submission_id,
            Uuid::new_v4(),
            now - chrono::Duration::minutes(2),
        ));
        root.reviewer_attestations = vec![
            review_attestation_fixture(&root, Uuid::new_v4(), 'a'),
            review_attestation_fixture(&root, Uuid::new_v4(), 'b'),
        ];
        let mut replacement = root.clone();
        replacement.evaluation_id = Uuid::new_v4();
        replacement.supersedes_evaluation_id = Some(root.evaluation_id);
        replacement.version = 2;
        replacement.created_at = now - chrono::Duration::minutes(1);
        replacement.evaluator_player_id = Uuid::new_v4();
        replacement.reviewer_attestations = vec![
            review_attestation_fixture(&replacement, Uuid::new_v4(), 'c'),
            review_attestation_fixture(&replacement, Uuid::new_v4(), 'd'),
        ];
        replacement.paper_score.evaluation_id = replacement.evaluation_id;
        replacement.paper_score.created_at = replacement.created_at;

        let evaluations = vec![root.clone(), replacement.clone()];
        let prepared = prepared_replacement_for_resolution(&root, evaluations.iter())
            .expect("one direct prepared replacement is unambiguous");
        ensure_appealed_evaluation_activated_for_resolution(
            &root,
            prepared,
            &evaluations,
            &[],
            &[],
        )
        .expect("the unique direct prepared child does not deactivate its parent before uphold");

        let mut dangling = replacement.clone();
        dangling.evaluation_id = Uuid::new_v4();
        dangling.supersedes_evaluation_id = Some(Uuid::new_v4());
        dangling.paper_score.evaluation_id = dangling.evaluation_id;
        let evaluations_with_dangling = vec![root.clone(), replacement.clone(), dangling];
        assert!(ensure_appealed_evaluation_activated_for_resolution(
            &root,
            Some(&replacement),
            &evaluations_with_dangling,
            &[],
            &[],
        )
        .is_err());

        let mut parallel = replacement.clone();
        parallel.evaluation_id = Uuid::new_v4();
        parallel.paper_score.evaluation_id = parallel.evaluation_id;
        let evaluations_with_parallel = [root.clone(), replacement, parallel];
        assert!(
            prepared_replacement_for_resolution(&root, evaluations_with_parallel.iter()).is_err()
        );
    }

    #[test]
    fn denied_resolution_cannot_omit_an_exact_prepared_replacement() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let root = evaluation_fixture_from_draft(&evaluation_draft_fixture(
            paper_id,
            submission_id,
            Uuid::new_v4(),
            Utc::now(),
        ));
        let mut replacement = root.clone();
        replacement.evaluation_id = Uuid::new_v4();
        replacement.supersedes_evaluation_id = Some(root.evaluation_id);
        replacement.version = 2;
        replacement.paper_score.evaluation_id = replacement.evaluation_id;

        let records = [root.clone(), replacement.clone()];
        let prepared = prepared_replacement_for_resolution(&root, records.iter())
            .expect("prepared replacement lineage is unambiguous");
        assert_eq!(
            prepared.map(|record| record.evaluation_id),
            Some(replacement.evaluation_id)
        );
        assert!(
            validate_resolution_replacement(&root, prepared, AppealOutcome::Denied, None,).is_err()
        );
        validate_resolution_replacement(
            &root,
            prepared,
            AppealOutcome::Upheld,
            Some(replacement.evaluation_id),
        )
        .expect("uphold binds the discovered prepared replacement");
        validate_resolution_replacement(&root, None, AppealOutcome::Denied, None)
            .expect("denial remains valid when no replacement was prepared");
    }

    #[test]
    fn assigned_review_raid_reproducer_is_fail_closed() {
        let paper_id = Uuid::new_v4();
        let assigned_player = Uuid::new_v4();
        let assignments = [assignment_fixture(
            paper_id,
            assigned_player,
            ReviewAssignmentSlot::Reproducer,
        )];
        let submission_id = assignments[0].submission_id;
        let now = Utc::now();
        enforce_reproducer_assignment(
            paper_id,
            submission_id,
            1,
            assigned_player,
            assignments.iter(),
            now,
        )
        .unwrap();
        assert!(enforce_reproducer_assignment(
            paper_id,
            submission_id,
            1,
            Uuid::new_v4(),
            assignments.iter(),
            now,
        )
        .is_err());
        assert!(enforce_reproducer_assignment(
            Uuid::new_v4(),
            submission_id,
            1,
            Uuid::new_v4(),
            assignments.iter(),
            now,
        )
        .is_err());
    }

    #[test]
    fn expired_review_assignment_reopens_its_round_slot() {
        let paper_id = Uuid::new_v4();
        let now = Utc::now();
        let mut expired =
            assignment_fixture(paper_id, Uuid::new_v4(), ReviewAssignmentSlot::Evaluator);
        expired.expires_at = now - chrono::Duration::seconds(1);
        let submission_id = expired.submission_id;
        let vacancies = claimable_review_vacancies(
            paper_id,
            submission_id,
            Uuid::new_v4(),
            &[],
            &[],
            &[],
            &[],
            &[expired],
            now,
        )
        .unwrap();
        assert!(vacancies.contains(&ReviewAssignmentVacancy {
            review_round: 1,
            slot: ReviewAssignmentSlot::Evaluator,
        }));
    }

    #[test]
    fn expired_draft_releases_only_its_pinned_panel_and_allows_replacement() {
        let paper_id = Uuid::new_v4();
        let evaluator_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();
        let now = Utc::now();
        let mut evaluator =
            assignment_fixture(paper_id, evaluator_id, ReviewAssignmentSlot::Evaluator);
        let submission_id = evaluator.submission_id;
        evaluator.expires_at = now + chrono::Duration::minutes(5);
        let evaluator_assignment_id = evaluator.assignment_id;
        let mut reviewer =
            assignment_fixture(paper_id, reviewer_id, ReviewAssignmentSlot::Reviewer1);
        reviewer.submission_id = submission_id;
        reviewer.expires_at = now + chrono::Duration::minutes(5);
        let reviewer_assignment_id = reviewer.assignment_id;
        let draft = evaluation_draft_fixture(paper_id, submission_id, evaluator_id, now);
        let evaluation_id = draft.evaluation_id;
        let draft_hash = draft.draft_hash.clone();
        let evaluation_signing_hash = draft.evaluation_signing_hash.clone();
        let mut memory = PaperRaidMemory::default();
        memory
            .review
            .assignments
            .insert(evaluator_assignment_id, evaluator);
        memory
            .review
            .assignments
            .insert(reviewer_assignment_id, reviewer);
        memory
            .review
            .evaluation_drafts
            .insert(evaluation_id, draft.clone());
        transition_review_assignment_memory(
            &mut memory.review,
            evaluator_assignment_id,
            ReviewAssignmentStatus::Pinned,
            Some(evaluation_id),
            now,
        )
        .expect("draft pins evaluator assignment");
        transition_review_assignment_memory(
            &mut memory.review,
            reviewer_assignment_id,
            ReviewAssignmentStatus::Pinned,
            Some(evaluation_id),
            now,
        )
        .expect("attestation pins reviewer assignment");
        let attestation_id = Uuid::new_v4();
        let old_attestation = EvaluationDraftAttestation {
            schema: EVALUATION_DRAFT_ATTESTATION_SCHEMA_V1.to_string(),
            attestation_id,
            evaluation_id,
            paper_project_id: paper_id,
            submission_id,
            review_round: 1,
            slot: ReviewAssignmentSlot::Reviewer1,
            draft_hash,
            attestation: ReviewAttestation {
                attestation_id,
                evaluation_id,
                evaluation_signing_hash,
                reviewer_player_id: reviewer_id,
                verdict: PanelVerdict::Approve,
                signing_key_id: "fixture-reviewer-key".to_string(),
                signing_public_key: "fixture-reviewer-public-key".to_string(),
                signing_public_key_hash: format!("sha256:{}", "1".repeat(64)),
                coi_attestation_hash: format!("sha256:{}", "2".repeat(64)),
                signed_at_unix: now.timestamp(),
                signature: "fixture-reviewer-signature".to_string(),
            },
            created_at: now,
        };
        memory
            .review
            .draft_attestations
            .insert(old_attestation.attestation_id, old_attestation.clone());

        let after_assignment_deadline = now + chrono::Duration::minutes(10);
        expire_review_assignments_memory(&mut memory.review, paper_id, after_assignment_deadline)
            .expect("expiry sweep ignores pinned assignment");
        expire_stale_evaluation_drafts_memory(
            &mut memory,
            paper_id,
            after_assignment_deadline,
            "fixture_claim",
            "fixture-before-deadline",
        )
        .expect("live draft remains pinned");
        let pinned = memory
            .review
            .assignments
            .get(&evaluator_assignment_id)
            .unwrap();
        assert_eq!(pinned.status, ReviewAssignmentStatus::Pinned);
        assert_eq!(pinned.version, 2);
        assert!(review_assignment_active(pinned, after_assignment_deadline));

        let after_draft_deadline = now + chrono::Duration::hours(2);
        expire_stale_evaluation_drafts_memory(
            &mut memory,
            paper_id,
            after_draft_deadline,
            "fixture_claim",
            "fixture-after-deadline",
        )
        .expect("expired draft atomically releases its exact pinned panel");
        let expired_draft = memory.review.evaluation_drafts.get(&evaluation_id).unwrap();
        assert_eq!(expired_draft.status, EvaluationDraftStatus::Expired);
        assert_eq!(expired_draft.version, 2);
        assert_eq!(expired_draft.expired_at, Some(draft.expires_at));
        assert!(ensure_evaluation_draft_lease_live(expired_draft, after_draft_deadline).is_err());
        for assignment_id in [evaluator_assignment_id, reviewer_assignment_id] {
            let expired = memory.review.assignments.get(&assignment_id).unwrap();
            assert_eq!(expired.status, ReviewAssignmentStatus::Expired);
            assert_eq!(expired.version, 3);
            assert_eq!(expired.pinned_evaluation_id, Some(evaluation_id));
            assert!(!review_assignment_active(expired, after_draft_deadline));
        }
        assert!(claimable_review_vacancies(
            paper_id,
            draft.submission_id,
            Uuid::new_v4(),
            &[],
            &[],
            &[],
            &[],
            &memory
                .review
                .assignments
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            after_draft_deadline,
        )
        .unwrap()
        .contains(&ReviewAssignmentVacancy {
            review_round: 1,
            slot: ReviewAssignmentSlot::Evaluator,
        }));
        let replacement = evaluation_draft_fixture(
            paper_id,
            submission_id,
            Uuid::new_v4(),
            after_draft_deadline,
        );
        ensure_evaluation_draft_slot_memory(
            &memory,
            paper_id,
            submission_id,
            1,
            replacement.evaluation_id,
        )
        .expect("expired historical draft does not reserve the active round");
        assert!(validate_draft_attestation_quorum(&replacement, &[old_attestation]).is_err());

        let mut finalized = replacement;
        finalized.status = EvaluationDraftStatus::Finalized;
        finalized.version = 2;
        finalized.finalized_evaluation_id = Some(finalized.evaluation_id);
        finalized.finalized_at = Some(after_draft_deadline);
        assert!(!expire_evaluation_draft(
            &mut finalized,
            after_draft_deadline + chrono::Duration::days(2)
        )
        .expect("finalized result never enters expiry lifecycle"));
        assert_eq!(finalized.status, EvaluationDraftStatus::Finalized);
    }

    #[test]
    fn evaluation_draft_assignment_requires_exact_active_round_and_slot() {
        let paper_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let now = Utc::now();
        let mut assignment =
            assignment_fixture(paper_id, player_id, ReviewAssignmentSlot::Reviewer1);
        let submission_id = assignment.submission_id;

        exact_active_assignment(
            paper_id,
            submission_id,
            1,
            player_id,
            ReviewAssignmentSlot::Reviewer1,
            std::iter::once(&assignment),
            now,
        )
        .expect("exact active assignment");
        assert!(exact_active_assignment(
            paper_id,
            submission_id,
            1,
            player_id,
            ReviewAssignmentSlot::Reviewer2,
            std::iter::once(&assignment),
            now,
        )
        .is_err());

        assignment.expires_at = now - chrono::Duration::seconds(1);
        assert!(exact_active_assignment(
            paper_id,
            submission_id,
            1,
            player_id,
            ReviewAssignmentSlot::Reviewer1,
            std::iter::once(&assignment),
            now,
        )
        .is_err());
        assignment.status = ReviewAssignmentStatus::Pinned;
        assignment.pinned_evaluation_id = Some(Uuid::new_v4());
        exact_active_assignment(
            paper_id,
            submission_id,
            1,
            player_id,
            ReviewAssignmentSlot::Reviewer1,
            std::iter::once(&assignment),
            now,
        )
        .expect("pinned assignment remains authoritative after its original deadline");
    }
}
