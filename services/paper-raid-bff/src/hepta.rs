use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::VerifyingKey;
use hepta_paper_raid_contracts::{
    authorship_consent_signing_bytes, canonical_json_bytes, human_decision_signing_bytes,
    human_evidence_verification_signing_bytes, human_key_registration_signing_bytes,
    paper_appeal_resolution_signing_bytes, paper_appeal_signing_bytes,
    paper_evaluation_signing_bytes, paper_reproduction_signing_bytes,
    paper_review_attestation_signing_bytes, paper_rework_signing_bytes,
    section_merge_signing_bytes, section_review_signing_bytes, sha256_digest,
    sign_consumer_user_assertion, team_member_acceptance_signing_bytes,
    verify_human_key_registration_pop, AuthorshipConsentSigningV2, ConsumerUserAssertionClaimV2,
    HumanDecisionSigningV1, HumanEvidenceVerificationSigningV1, HumanKeyRegistrationClaimV2,
    PaperAppealResolutionSigningV1, PaperAppealSigningV1, PaperEvaluationSigningV1,
    PaperReproductionSigningV1, PaperReviewAttestationSigningV1, PaperReworkSigningV1,
    SectionMergeSigningV1, SectionReviewSigningV1, TeamMemberAcceptanceSigningV2,
    AUTHORSHIP_CONSENT_V2, CONSUMER_USER_ASSERTION_V2, HUMAN_DECISION_V1,
    HUMAN_EVIDENCE_VERIFICATION_V1, HUMAN_KEY_REGISTRATION_V2, PAPER_APPEAL_RESOLUTION_V1,
    PAPER_APPEAL_V1, PAPER_EVALUATION_V1, PAPER_REPRODUCTION_V1, PAPER_REVIEW_ATTESTATION_V1,
    PAPER_REWORK_V1, SECTION_MERGE_V1, SECTION_REVIEW_V1, TEAM_MEMBER_ACCEPTANCE_V2,
};
use reqwest::{header, Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use url::Url;
use uuid::Uuid;

use crate::{
    config::{AlphaAuthorRole, AlphaIdentity, AlphaIdentityScope, ConsumerAssertionConfig},
    error::AppError,
    metrics::{HeptaErrorKind, Metrics},
};

const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;
const ASSERTION_HEADER: &str = "x-hepta-user-assertion";
const HUMAN_REGISTRATION_BUCKET_SECONDS: i64 = 300;
const HUMAN_REGISTRATION_TTL_SECONDS: i64 = 600;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandName {
    RotateHumanSigningKey,
    RevokeHumanSigningKey,
    CreateAgentBinding,
    RotateAgentBindingKey,
    CreateResearchTeam,
    AcceptResearchTeamMembership,
    LockResearchTeam,
    CreatePaperProject,
    TransitionPaperChallengeOutcome,
    TransitionPaperProject,
    CreatePaperWorkItem,
    TransitionPaperWorkItem,
    CreatePaperRevision,
    PromotePaperReleaseCandidate,
    CreateAuthorshipConsent,
    FinalizeJointPaperSubmission,
    StartPaperRework,
    IssueResearchSessionAuthorizationSet,
    ReplaceResearchSessionAuthorizationSet,
    CreateNakamaResearchSessionControl,
    ResumeNakamaResearchSessionControl,
    ReplaceNakamaResearchSessionRosterControl,
    CompleteNakamaResearchSessionControl,
    QueueMatchmaking,
    CancelMatchmakingTicket,
    DecideTeamProposal,
    MaterializeTeamProposal,
    CreateEvidenceCard,
    CreateCitationRecord,
    CreateExperimentPlan,
    CreateRunRecord,
    CreateRoleResourceAction,
    CreateFigureLineage,
    CreateClaimRecord,
    AcquireSectionLease,
    SubmitAgentProposal,
    RecordHumanDecision,
    RegisterArtifact,
    CreateSectionRevision,
    SubmitReview,
    MergeSection,
    CreateContributionLedger,
    ClaimReviewAssignment,
    CreatePaperEvaluation,
    CreatePaperEvaluationDraft,
    SubmitEvaluationDraftAttestation,
    FinalizePaperEvaluationDraft,
    SubmitReproduction,
    SubmitAppeal,
    ResolveAppeal,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanSigningFrameRequest {
    pub command: CommandName,
    pub resource_id: Option<Uuid>,
    pub child_id: Option<Uuid>,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
pub struct HumanSigningFrameResponse {
    pub command: CommandName,
    pub resource_id: Option<Uuid>,
    pub child_id: Option<Uuid>,
    pub payload: Value,
    pub signing_bytes: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
}

#[derive(Debug, Deserialize)]
struct PlayerSnapshot {
    player_id: Uuid,
    subject_id: String,
    signing_key_id: String,
    signing_public_key: String,
    signing_public_key_hash: String,
}

#[derive(Debug, Deserialize)]
struct TeamSigningSnapshot {
    team_id: Uuid,
    challenge_id: Uuid,
    version: u64,
    roster_version: u64,
    collaboration_compact_hash: String,
    members: Vec<TeamMemberSigningSnapshot>,
}

#[derive(Debug, Deserialize)]
struct TeamMemberSigningSnapshot {
    participant_slot: u32,
    player_id: Uuid,
    binding_id: Uuid,
    agent_id: String,
    role: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceFramePayload {
    acceptance_id: Uuid,
    expected_team_version: u64,
    roster_version: u64,
    participant_slot: u32,
    binding_id: Uuid,
    role: String,
    collaboration_compact_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewFramePayload {
    review_id: Uuid,
    expected_revision_version: u64,
    verdict: String,
    review_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanDecisionFramePayload {
    decision_id: Uuid,
    proposal_id: Uuid,
    expected_proposal_version: u64,
    decision: String,
    reason_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionMergeFramePayload {
    merge_id: Uuid,
    section_revision_id: Uuid,
    expected_revision_version: u64,
    parent_revision_id: Uuid,
    merged_section_revision_id: Uuid,
    lease_id: Uuid,
    fencing_token: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsentFramePayload {
    consent_id: Uuid,
    expected_paper_version: u64,
    revision_id: Uuid,
    release_candidate_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppealFramePayload {
    appeal_id: Uuid,
    grounds_hash: String,
    #[serde(default)]
    evidence_manifest_id: Option<Uuid>,
    #[serde(default)]
    release_candidate_hash: Option<String>,
    #[serde(default)]
    evidence_manifest_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperReworkFramePayload {
    rework_id: Uuid,
    reason_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppealResolutionFramePayload {
    resolution_id: Uuid,
    outcome: String,
    decision_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvaluationScoreComponentsInput {
    method_rigor_bps: u16,
    experiment_statistics_bps: u16,
    reproducibility_bps: u16,
    evidence_citations_bps: u16,
    value_originality_bps: u16,
    argument_expression_bps: u16,
    ethics_transparency_bps: u16,
}

impl EvaluationScoreComponentsInput {
    fn checked_total(&self) -> Result<u16, AppError> {
        for (field, value, maximum) in [
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
            if value > maximum {
                return Err(AppError::Invalid(format!(
                    "{field} exceeds the frozen {maximum} bps maximum"
                )));
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EvaluationHardGatesInput {
    citations_and_data_authentic: bool,
    failed_runs_disclosed: bool,
    all_authors_consented: bool,
    core_claims_have_evidence: bool,
    artifact_lineage_complete: bool,
    license_ethics_coi_complete: bool,
}

impl EvaluationHardGatesInput {
    fn eligible(&self) -> bool {
        self.citations_and_data_authentic
            && self.failed_runs_disclosed
            && self.all_authors_consented
            && self.core_claims_have_evidence
            && self.artifact_lineage_complete
            && self.license_ethics_coi_complete
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationDraftFramePayload {
    evaluation_id: Uuid,
    supersedes_evaluation_id: Option<Uuid>,
    tolerance_policy: Value,
    reference_metrics_micros: BTreeMap<String, i64>,
    score_components: EvaluationScoreComponentsInput,
    hard_gates: EvaluationHardGatesInput,
    evaluator_coi_attestation_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluationAttestationFramePayload {
    attestation_id: Uuid,
    verdict: String,
    coi_attestation_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReproductionFramePayload {
    reproduction_id: Uuid,
    supersedes_reproduction_id: Option<Uuid>,
    observed_metrics_micros: BTreeMap<String, i64>,
    statistical_evidence: BTreeMap<String, Value>,
    seed_set_hash: String,
    environment_hash: String,
    run_manifest_hash: String,
    coi_attestation_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceVerificationFramePayload {
    evidence_card_id: Uuid,
    source_uri: String,
    source_hash: String,
    locator: String,
    license: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CitationVerificationFramePayload {
    citation_id: Uuid,
    evidence_card_id: Uuid,
    doi: Option<String>,
    canonical_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserCommand {
    pub command: CommandName,
    pub resource_id: Option<Uuid>,
    pub child_id: Option<Uuid>,
    pub session_id: Option<String>,
    pub idempotency_key: Uuid,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct HumanRegistrationChallenge {
    pub schema: String,
    pub player_id: Uuid,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub display_name: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub idempotency_key: Uuid,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub signing_bytes: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanRegistrationProof {
    pub signing_public_key: String,
    pub idempotency_key: Uuid,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub key_proof_signature: String,
}

#[derive(Debug, Clone)]
struct Route {
    method: Method,
    path: String,
    query: Option<String>,
    operation: &'static str,
}

impl Route {
    fn canonical_path(&self) -> String {
        match &self.query {
            Some(query) => format!("{}?{query}", self.path),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HeptaPaperTerminalOutcome {
    Failed,
    Expired,
    Abandoned,
}

impl HeptaPaperTerminalOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeptaPaperTerminalReason {
    QualityGateFailed,
    PreregisteredResultFailed,
    IntegrityFailure,
    TeamWithdrawal,
    ResourceUnavailable,
    ChallengeInfeasible,
    ChallengeGraceDeadlineElapsed,
}

impl HeptaPaperTerminalReason {
    pub(crate) fn parse_for_outcome(
        outcome: HeptaPaperTerminalOutcome,
        value: &str,
    ) -> Option<Self> {
        match (outcome, value) {
            (HeptaPaperTerminalOutcome::Failed, "quality_gate_failed") => {
                Some(Self::QualityGateFailed)
            }
            (HeptaPaperTerminalOutcome::Failed, "preregistered_result_failed") => {
                Some(Self::PreregisteredResultFailed)
            }
            (HeptaPaperTerminalOutcome::Failed, "integrity_failure") => {
                Some(Self::IntegrityFailure)
            }
            (HeptaPaperTerminalOutcome::Abandoned, "team_withdrawal") => Some(Self::TeamWithdrawal),
            (HeptaPaperTerminalOutcome::Abandoned, "resource_unavailable") => {
                Some(Self::ResourceUnavailable)
            }
            (HeptaPaperTerminalOutcome::Abandoned, "challenge_infeasible") => {
                Some(Self::ChallengeInfeasible)
            }
            (HeptaPaperTerminalOutcome::Expired, "challenge_grace_deadline_elapsed") => {
                Some(Self::ChallengeGraceDeadlineElapsed)
            }
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::QualityGateFailed => "quality_gate_failed",
            Self::PreregisteredResultFailed => "preregistered_result_failed",
            Self::IntegrityFailure => "integrity_failure",
            Self::TeamWithdrawal => "team_withdrawal",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::ChallengeInfeasible => "challenge_infeasible",
            Self::ChallengeGraceDeadlineElapsed => "challenge_grace_deadline_elapsed",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperRoomEnvelopeV3 {
    paper: Value,
    team: Value,
    author_raid_progress: Value,
    team_member_acceptances: Vec<Value>,
    work_items: Vec<Value>,
    paper_revisions: Vec<Value>,
    authorship_consents: Vec<Value>,
    joint_submission: Option<Value>,
    member_research_sessions: Vec<Value>,
    artifact_manifests: Vec<Value>,
    revision_artifact_bindings: Vec<Value>,
    evidence_cards: Vec<Value>,
    citations: Vec<Value>,
    experiment_plans: Vec<Value>,
    runs: Vec<Value>,
    figures: Vec<Value>,
    claims: Vec<Value>,
    section_heads: Vec<Value>,
    leases: Vec<Value>,
    proposals: Vec<Value>,
    decisions: Vec<Value>,
    section_revisions: Vec<Value>,
    section_reviews: Vec<Value>,
    section_merges: Vec<Value>,
    last_event_cursor: u64,
}

impl PaperRoomEnvelopeV3 {
    fn consume_for_strict_shape(self) {
        let Self {
            paper,
            team,
            author_raid_progress,
            team_member_acceptances,
            work_items,
            paper_revisions,
            authorship_consents,
            joint_submission,
            member_research_sessions,
            artifact_manifests,
            revision_artifact_bindings,
            evidence_cards,
            citations,
            experiment_plans,
            runs,
            figures,
            claims,
            section_heads,
            leases,
            proposals,
            decisions,
            section_revisions,
            section_reviews,
            section_merges,
            last_event_cursor,
        } = self;
        drop((
            paper,
            team,
            author_raid_progress,
            team_member_acceptances,
            work_items,
            paper_revisions,
            authorship_consents,
            joint_submission,
            member_research_sessions,
            artifact_manifests,
            revision_artifact_bindings,
            evidence_cards,
            citations,
            experiment_plans,
            runs,
            figures,
            claims,
            section_heads,
            leases,
            proposals,
            decisions,
            section_revisions,
            section_reviews,
            section_merges,
            last_event_cursor,
        ));
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SealedConsumerFinalityStatusV2 {
    PendingFinality,
    VerifiedFinality,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedConsumerFinalityV2 {
    schema: String,
    status: SealedConsumerFinalityStatusV2,
    effective_evaluation_id: Option<Uuid>,
    effective_reproduction_id: Option<Uuid>,
    effective_appeal_resolution_id: Option<Uuid>,
    ranking_eligible: bool,
    reward_eligible: bool,
    score_eligible: bool,
    economic_eligible: bool,
    verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
enum SealedReviewAssignmentSlotV1 {
    Evaluator,
    #[serde(rename = "reviewer_1")]
    Reviewer1,
    #[serde(rename = "reviewer_2")]
    Reviewer2,
    Reproducer,
}

impl SealedReviewAssignmentSlotV1 {
    fn rank(self) -> u8 {
        match self {
            Self::Evaluator => 0,
            Self::Reviewer1 => 1,
            Self::Reviewer2 => 2,
            Self::Reproducer => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SealedReviewAssignmentStatusV1 {
    Claimed,
    Pinned,
    Consumed,
    Expired,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedReviewAssignmentV1 {
    schema: String,
    assignment_id: Uuid,
    paper_project_id: Uuid,
    submission_id: Uuid,
    player_id: Uuid,
    review_round: u64,
    slot: SealedReviewAssignmentSlotV1,
    pinned_evaluation_id: Option<Uuid>,
    status: SealedReviewAssignmentStatusV1,
    version: u64,
    claimed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SealedEvaluationDraftStatusV1 {
    Open,
    Finalized,
    Expired,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedEvaluationDraftV1 {
    schema: String,
    evaluation_id: Uuid,
    paper_project_id: Uuid,
    submission_id: Uuid,
    review_round: u64,
    supersedes_evaluation_id: Option<Uuid>,
    release_candidate_hash: String,
    paper_bundle_hash: String,
    tolerance_policy: Value,
    tolerance_policy_hash: String,
    reference_metrics_micros: BTreeMap<String, i64>,
    paper_score: Value,
    evaluator_player_id: Uuid,
    evaluator_signing_key_id: String,
    evaluator_signing_public_key: String,
    evaluator_signing_public_key_hash: String,
    evaluator_coi_attestation_hash: String,
    evaluator_signed_at_unix: i64,
    evaluator_signature: String,
    evaluation_signing_hash: String,
    draft_hash: String,
    status: SealedEvaluationDraftStatusV1,
    version: u64,
    finalized_evaluation_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    finalized_at: Option<DateTime<Utc>>,
    expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedStatisticalEvidenceV1 {
    interval_overlap_bps: u16,
    effect_delta_micros: i64,
    p_value_micros: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedReproductionRuleResultV1 {
    rule_key: String,
    passed: bool,
    detail_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SealedReproductionStatusV1 {
    Reproduced,
    FailedTolerance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedPaperReproductionV1 {
    schema: String,
    reproduction_id: Uuid,
    evaluation_id: Uuid,
    paper_project_id: Uuid,
    release_candidate_hash: String,
    paper_bundle_hash: String,
    tolerance_policy_hash: String,
    observed_metrics_micros: BTreeMap<String, i64>,
    statistical_evidence: BTreeMap<String, SealedStatisticalEvidenceV1>,
    seed_set_hash: String,
    environment_hash: String,
    run_manifest_hash: String,
    supersedes_reproduction_id: Option<Uuid>,
    reproducer_player_id: Uuid,
    signing_key_id: String,
    signing_public_key: String,
    signing_public_key_hash: String,
    coi_attestation_hash: String,
    signed_at_unix: i64,
    signature: String,
    rule_results: Vec<SealedReproductionRuleResultV1>,
    status: SealedReproductionStatusV1,
    report_hash: String,
    version: u64,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperReviewStateEnvelopeV1 {
    finality: SealedConsumerFinalityV2,
    assignments: Vec<SealedReviewAssignmentV1>,
    evaluation_drafts: Vec<SealedEvaluationDraftV1>,
    contribution_ledgers: Vec<Value>,
    evaluations: Vec<Value>,
    reproductions: Vec<SealedPaperReproductionV1>,
    appeals: Vec<Value>,
    resolutions: Vec<Value>,
    raid_scores: Vec<Value>,
}

#[derive(Clone)]
struct SealedEvaluationCrosslinkV1 {
    submission_id: Uuid,
    version: u64,
    release_candidate_hash: String,
    paper_bundle_hash: String,
    tolerance_policy_hash: String,
    evaluator_player_id: Uuid,
    reviewer_player_ids: HashSet<Uuid>,
    tolerance_rule_keys: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Clone)]
struct SealedEvaluationDraftCrosslinkV1 {
    submission_id: Uuid,
    review_round: u64,
    evaluator_player_id: Uuid,
    status: SealedEvaluationDraftStatusV1,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn sealed_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn sealed_nonzero_digest(value: &str) -> bool {
    sealed_digest(value) && value != format!("sha256:{}", "0".repeat(64))
}

fn sealed_text(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 512 && !value.as_bytes().contains(&0)
}

fn sealed_canonical_base64(value: &str, expected_len: usize) -> bool {
    BASE64
        .decode(value)
        .ok()
        .is_some_and(|decoded| decoded.len() == expected_len && BASE64.encode(decoded) == value)
}

fn sealed_public_key(public_key: &str, public_key_hash: &str) -> bool {
    BASE64.decode(public_key).ok().is_some_and(|decoded| {
        decoded.len() == 32
            && BASE64.encode(&decoded) == public_key
            && sha256_digest(&decoded) == public_key_hash
    })
}

fn sealed_uuid(record: &Value, field: &str, expected: Uuid) -> bool {
    !expected.is_nil()
        && record.get(field).and_then(Value::as_str) == Some(expected.to_string().as_str())
}

fn sealed_optional_uuid(record: &Value, field: &str, expected: Option<Uuid>) -> bool {
    match (record.get(field), expected) {
        (Some(Value::Null), None) => true,
        (Some(Value::String(value)), Some(expected)) => {
            !expected.is_nil() && value == &expected.to_string()
        }
        _ => false,
    }
}

fn sealed_timestamp(record: &Value, field: &str, expected: DateTime<Utc>) -> bool {
    record.get(field).and_then(Value::as_str)
        == Some(
            expected
                .to_rfc3339_opts(SecondsFormat::AutoSi, true)
                .as_str(),
        )
}

fn sealed_optional_timestamp(record: &Value, field: &str, expected: Option<DateTime<Utc>>) -> bool {
    match (record.get(field), expected) {
        (Some(Value::Null), None) => true,
        (Some(Value::String(value)), Some(expected)) => {
            value == &expected.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        }
        _ => false,
    }
}

fn sealed_tolerance_rule_keys(evaluation: &Value) -> Option<Vec<String>> {
    let reference = evaluation.get("reference_metrics_micros")?.as_object()?;
    if reference.is_empty() || reference.len() > 256 {
        return None;
    }
    let rules = evaluation
        .get("tolerance_policy")?
        .get("rules")?
        .as_array()?;
    if rules.is_empty() || rules.len() > 128 {
        return None;
    }
    let mut seen = HashSet::with_capacity(rules.len());
    let mut keys = Vec::with_capacity(rules.len());
    for rule in rules {
        let rule = rule.as_object()?;
        let key = match rule.get("kind")?.as_str()? {
            "absolute" => format!("absolute:{}", rule.get("metric")?.as_str()?),
            "relative" => format!("relative:{}", rule.get("metric")?.as_str()?),
            "statistical" => format!("statistical:{}", rule.get("metric")?.as_str()?),
            "seed" => format!("seed:{}", rule.get("expected_seed_set_hash")?.as_str()?),
            _ => return None,
        };
        if !sealed_text(&key) || !seen.insert(key.clone()) {
            return None;
        }
        keys.push(key);
    }
    Some(keys)
}

fn sealed_evaluation_crosslinks(
    records: &[Value],
    paper_id: Uuid,
) -> Option<HashMap<Uuid, SealedEvaluationCrosslinkV1>> {
    let paper_id_text = paper_id.to_string();
    let mut result = HashMap::with_capacity(records.len());
    for record in records {
        if record.get("schema")?.as_str()? != "hepta.paper_raid.evaluation.v1"
            || record.get("paper_project_id")?.as_str()? != paper_id_text
        {
            return None;
        }
        let evaluation_id = Uuid::parse_str(record.get("evaluation_id")?.as_str()?).ok()?;
        let submission_id = Uuid::parse_str(record.get("submission_id")?.as_str()?).ok()?;
        let evaluator_player_id =
            Uuid::parse_str(record.get("evaluator_player_id")?.as_str()?).ok()?;
        if !sealed_uuid(record, "evaluation_id", evaluation_id)
            || !sealed_uuid(record, "submission_id", submission_id)
            || !sealed_uuid(record, "evaluator_player_id", evaluator_player_id)
        {
            return None;
        }
        let version = record.get("version")?.as_u64()?;
        let release_candidate_hash = record.get("release_candidate_hash")?.as_str()?;
        let paper_bundle_hash = record.get("paper_bundle_hash")?.as_str()?;
        let tolerance_policy_hash = record.get("tolerance_policy_hash")?.as_str()?;
        let created_at = DateTime::parse_from_rfc3339(record.get("created_at")?.as_str()?)
            .ok()?
            .with_timezone(&Utc);
        if version == 0
            || !sealed_digest(release_candidate_hash)
            || !sealed_digest(paper_bundle_hash)
            || !sealed_digest(tolerance_policy_hash)
            || !sealed_timestamp(record, "created_at", created_at)
        {
            return None;
        }
        let reviewers = record.get("reviewer_attestations")?.as_array()?;
        if reviewers.len() != 2 {
            return None;
        }
        let reviewer_player_ids = reviewers
            .iter()
            .map(|review| {
                let reviewer = Uuid::parse_str(review.get("reviewer_player_id")?.as_str()?).ok()?;
                sealed_uuid(review, "reviewer_player_id", reviewer).then_some(reviewer)
            })
            .collect::<Option<HashSet<_>>>()?;
        if reviewer_player_ids.len() != 2 || reviewer_player_ids.contains(&evaluator_player_id) {
            return None;
        }
        let crosslink = SealedEvaluationCrosslinkV1 {
            submission_id,
            version,
            release_candidate_hash: release_candidate_hash.into(),
            paper_bundle_hash: paper_bundle_hash.into(),
            tolerance_policy_hash: tolerance_policy_hash.into(),
            evaluator_player_id,
            reviewer_player_ids,
            tolerance_rule_keys: sealed_tolerance_rule_keys(record)?,
            created_at,
        };
        if result.insert(evaluation_id, crosslink).is_some() {
            return None;
        }
    }
    Some(result)
}

fn sealed_evaluation_draft_hash(record: &Value, schema: &str) -> Option<String> {
    let fields = match schema {
        "hepta.paper_raid.evaluation_draft.v1" => &[
            "schema",
            "evaluation_id",
            "paper_project_id",
            "submission_id",
            "review_round",
            "supersedes_evaluation_id",
            "release_candidate_hash",
            "paper_bundle_hash",
            "tolerance_policy",
            "tolerance_policy_hash",
            "reference_metrics_micros",
            "paper_score",
            "evaluator_player_id",
            "evaluator_signing_key_id",
            "evaluator_signing_public_key",
            "evaluator_signing_public_key_hash",
            "evaluator_coi_attestation_hash",
            "evaluator_signed_at_unix",
            "evaluator_signature",
            "evaluation_signing_hash",
        ][..],
        "hepta.paper_raid.evaluation_draft.v2" => &[
            "schema",
            "evaluation_id",
            "paper_project_id",
            "submission_id",
            "review_round",
            "supersedes_evaluation_id",
            "release_candidate_hash",
            "paper_bundle_hash",
            "tolerance_policy",
            "tolerance_policy_hash",
            "reference_metrics_micros",
            "paper_score",
            "evaluator_player_id",
            "evaluator_signing_key_id",
            "evaluator_signing_public_key",
            "evaluator_signing_public_key_hash",
            "evaluator_coi_attestation_hash",
            "evaluator_signed_at_unix",
            "evaluator_signature",
            "evaluation_signing_hash",
            "expires_at",
        ][..],
        _ => return None,
    };
    let source = record.as_object()?;
    let mut immutable = serde_json::Map::with_capacity(fields.len());
    for field in fields {
        immutable.insert((*field).into(), source.get(*field)?.clone());
    }
    Some(sha256_digest(
        &canonical_json_bytes(&Value::Object(immutable)).ok()?,
    ))
}

fn sealed_evaluation_draft_crosslinks(
    raw: &[Value],
    drafts: &[SealedEvaluationDraftV1],
    paper_id: Uuid,
    evaluations: &HashMap<Uuid, SealedEvaluationCrosslinkV1>,
) -> Option<HashMap<Uuid, SealedEvaluationDraftCrosslinkV1>> {
    if raw.len() != drafts.len() || drafts.len() > 4_096 {
        return None;
    }
    let mut result = HashMap::with_capacity(drafts.len());
    let mut open_rounds = HashSet::with_capacity(drafts.len());
    let mut previous_id = None;
    for (raw, draft) in raw.iter().zip(drafts) {
        if raw.as_object()?.len() != 29
            || !matches!(
                draft.schema.as_str(),
                "hepta.paper_raid.evaluation_draft.v1" | "hepta.paper_raid.evaluation_draft.v2"
            )
            || draft.paper_project_id != paper_id
            || draft.review_round == 0
            || draft.created_at >= draft.expires_at
            || draft.evaluator_signed_at_unix < 0
            || draft.reference_metrics_micros.is_empty()
            || draft.reference_metrics_micros.len() > 256
            || draft
                .reference_metrics_micros
                .keys()
                .any(|key| !sealed_text(key))
            || !sealed_uuid(raw, "evaluation_id", draft.evaluation_id)
            || !sealed_uuid(raw, "paper_project_id", draft.paper_project_id)
            || !sealed_uuid(raw, "submission_id", draft.submission_id)
            || !sealed_optional_uuid(
                raw,
                "supersedes_evaluation_id",
                draft.supersedes_evaluation_id,
            )
            || !sealed_uuid(raw, "evaluator_player_id", draft.evaluator_player_id)
            || !sealed_optional_uuid(
                raw,
                "finalized_evaluation_id",
                draft.finalized_evaluation_id,
            )
            || !sealed_timestamp(raw, "created_at", draft.created_at)
            || !sealed_timestamp(raw, "expires_at", draft.expires_at)
            || !sealed_timestamp(raw, "updated_at", draft.updated_at)
            || !sealed_optional_timestamp(raw, "finalized_at", draft.finalized_at)
            || !sealed_optional_timestamp(raw, "expired_at", draft.expired_at)
            || previous_id.is_some_and(|previous| previous >= draft.evaluation_id)
            || sealed_tolerance_rule_keys(raw).is_none()
        {
            return None;
        }
        previous_id = Some(draft.evaluation_id);
        for digest in [
            &draft.release_candidate_hash,
            &draft.paper_bundle_hash,
            &draft.tolerance_policy_hash,
            &draft.evaluator_signing_public_key_hash,
            &draft.evaluator_coi_attestation_hash,
            &draft.evaluation_signing_hash,
            &draft.draft_hash,
        ] {
            if !sealed_digest(digest) {
                return None;
            }
        }
        if !sealed_text(&draft.evaluator_signing_key_id)
            || !sealed_public_key(
                &draft.evaluator_signing_public_key,
                &draft.evaluator_signing_public_key_hash,
            )
            || !sealed_canonical_base64(&draft.evaluator_signature, 64)
            || sha256_digest(&canonical_json_bytes(&draft.tolerance_policy).ok()?)
                != draft.tolerance_policy_hash
            || sealed_evaluation_draft_hash(raw, &draft.schema)? != draft.draft_hash
        {
            return None;
        }
        let paper_score = draft.paper_score.as_object()?;
        if paper_score.len() != 9
            || paper_score.get("schema")?.as_str()? != "hepta.paper_raid.paper_score.v1"
            || !sealed_uuid(&draft.paper_score, "evaluation_id", draft.evaluation_id)
            || !sealed_uuid(
                &draft.paper_score,
                "paper_project_id",
                draft.paper_project_id,
            )
            || !sealed_timestamp(&draft.paper_score, "created_at", draft.created_at)
            || !sealed_digest(paper_score.get("score_hash")?.as_str()?)
        {
            return None;
        }
        let lifecycle_valid = match draft.status {
            SealedEvaluationDraftStatusV1::Open => {
                draft.version == 1
                    && draft.finalized_evaluation_id.is_none()
                    && draft.updated_at == draft.created_at
                    && draft.finalized_at.is_none()
                    && draft.expired_at.is_none()
                    && !evaluations.contains_key(&draft.evaluation_id)
                    && open_rounds.insert((draft.submission_id, draft.review_round))
            }
            SealedEvaluationDraftStatusV1::Expired => {
                draft.version == 2
                    && draft.finalized_evaluation_id.is_none()
                    && draft.updated_at == draft.expires_at
                    && draft.finalized_at.is_none()
                    && draft.expired_at == Some(draft.expires_at)
                    && !evaluations.contains_key(&draft.evaluation_id)
            }
            // Finalized drafts are deliberately omitted from review-state;
            // consumed assignments remain linked to finalized evaluations.
            SealedEvaluationDraftStatusV1::Finalized => false,
        };
        if !lifecycle_valid
            || result
                .insert(
                    draft.evaluation_id,
                    SealedEvaluationDraftCrosslinkV1 {
                        submission_id: draft.submission_id,
                        review_round: draft.review_round,
                        evaluator_player_id: draft.evaluator_player_id,
                        status: draft.status,
                        created_at: draft.created_at,
                        expires_at: draft.expires_at,
                        updated_at: draft.updated_at,
                    },
                )
                .is_some()
        {
            return None;
        }
    }
    Some(result)
}

fn sealed_review_assignments(
    raw: &[Value],
    assignments: &[SealedReviewAssignmentV1],
    paper_id: Uuid,
    evaluations: &HashMap<Uuid, SealedEvaluationCrosslinkV1>,
    evaluation_drafts: &HashMap<Uuid, SealedEvaluationDraftCrosslinkV1>,
) -> Option<()> {
    if raw.len() != assignments.len() || assignments.len() > 16_384 {
        return None;
    }
    let mut ids = HashSet::with_capacity(assignments.len());
    let mut live_slots = HashSet::with_capacity(assignments.len());
    let mut live_players = HashSet::with_capacity(assignments.len());
    let mut draft_evaluators = HashMap::<Uuid, usize>::new();
    let mut previous = None;
    let mut consumed = HashMap::<Uuid, Vec<&SealedReviewAssignmentV1>>::new();
    for (raw, assignment) in raw.iter().zip(assignments) {
        if raw.as_object()?.len() != 13
            || assignment.schema != "hepta.paper_raid.review_assignment.v1"
            || assignment.paper_project_id != paper_id
            || assignment.review_round == 0
            || assignment.claimed_at >= assignment.expires_at
            || assignment.updated_at < assignment.claimed_at
            || !sealed_uuid(raw, "assignment_id", assignment.assignment_id)
            || !sealed_uuid(raw, "paper_project_id", assignment.paper_project_id)
            || !sealed_uuid(raw, "submission_id", assignment.submission_id)
            || !sealed_uuid(raw, "player_id", assignment.player_id)
            || !sealed_optional_uuid(raw, "pinned_evaluation_id", assignment.pinned_evaluation_id)
            || !sealed_timestamp(raw, "claimed_at", assignment.claimed_at)
            || !sealed_timestamp(raw, "expires_at", assignment.expires_at)
            || !sealed_timestamp(raw, "updated_at", assignment.updated_at)
            || !ids.insert(assignment.assignment_id)
        {
            return None;
        }
        let ordering = (
            assignment.review_round,
            assignment.slot.rank(),
            assignment.assignment_id,
        );
        if previous.is_some_and(|previous| previous >= ordering) {
            return None;
        }
        previous = Some(ordering);
        let lifecycle_valid = match assignment.status {
            SealedReviewAssignmentStatusV1::Claimed => {
                assignment.version == 1
                    && assignment.pinned_evaluation_id.is_none()
                    && assignment.updated_at == assignment.claimed_at
            }
            SealedReviewAssignmentStatusV1::Pinned => {
                assignment.version == 2
                    && assignment.pinned_evaluation_id.is_some()
                    && assignment.updated_at < assignment.expires_at
            }
            SealedReviewAssignmentStatusV1::Consumed => {
                assignment.version == 3 && assignment.pinned_evaluation_id.is_some()
            }
            SealedReviewAssignmentStatusV1::Expired => match assignment.pinned_evaluation_id {
                None => assignment.version == 2 && assignment.updated_at >= assignment.expires_at,
                Some(_) => assignment.version == 3,
            },
        };
        if !lifecycle_valid
            || (assignment.pinned_evaluation_id.is_some()
                && assignment.slot == SealedReviewAssignmentSlotV1::Reproducer)
        {
            return None;
        }
        if matches!(
            assignment.status,
            SealedReviewAssignmentStatusV1::Claimed | SealedReviewAssignmentStatusV1::Pinned
        ) && (!live_slots.insert((assignment.review_round, assignment.slot))
            || !live_players.insert((assignment.review_round, assignment.player_id)))
        {
            return None;
        }
        if let Some(evaluation_id) = assignment.pinned_evaluation_id {
            if assignment.status == SealedReviewAssignmentStatusV1::Consumed {
                let evaluation = evaluations.get(&evaluation_id)?;
                if assignment.submission_id != evaluation.submission_id
                    || assignment.review_round != evaluation.version
                {
                    return None;
                }
                if assignment.updated_at != evaluation.created_at {
                    return None;
                }
                consumed.entry(evaluation_id).or_default().push(assignment);
            } else {
                let draft = evaluation_drafts.get(&evaluation_id)?;
                let expected_status = match assignment.status {
                    SealedReviewAssignmentStatusV1::Pinned => SealedEvaluationDraftStatusV1::Open,
                    SealedReviewAssignmentStatusV1::Expired => {
                        SealedEvaluationDraftStatusV1::Expired
                    }
                    _ => return None,
                };
                if draft.status != expected_status
                    || assignment.submission_id != draft.submission_id
                    || assignment.review_round != draft.review_round
                    || assignment.claimed_at >= draft.expires_at
                    || (assignment.slot == SealedReviewAssignmentSlotV1::Evaluator
                        && assignment.claimed_at > draft.created_at)
                    || (assignment.status == SealedReviewAssignmentStatusV1::Pinned
                        && (assignment.updated_at < draft.created_at
                            || assignment.updated_at >= draft.expires_at))
                    || (assignment.status == SealedReviewAssignmentStatusV1::Expired
                        && assignment.updated_at != draft.updated_at)
                {
                    return None;
                }
                if assignment.slot == SealedReviewAssignmentSlotV1::Evaluator {
                    if assignment.player_id != draft.evaluator_player_id {
                        return None;
                    }
                    *draft_evaluators.entry(evaluation_id).or_default() += 1;
                }
            }
        }
    }
    if evaluation_drafts
        .keys()
        .any(|evaluation_id| draft_evaluators.get(evaluation_id) != Some(&1))
    {
        return None;
    }
    for (evaluation_id, evaluation) in evaluations {
        let panel = consumed.get(evaluation_id)?;
        if panel.len() != 3 {
            return None;
        }
        let evaluator = panel
            .iter()
            .filter(|assignment| assignment.slot == SealedReviewAssignmentSlotV1::Evaluator)
            .map(|assignment| assignment.player_id)
            .collect::<Vec<_>>();
        let reviewers = panel
            .iter()
            .filter(|assignment| {
                matches!(
                    assignment.slot,
                    SealedReviewAssignmentSlotV1::Reviewer1
                        | SealedReviewAssignmentSlotV1::Reviewer2
                )
            })
            .map(|assignment| assignment.player_id)
            .collect::<HashSet<_>>();
        let slots = panel
            .iter()
            .map(|assignment| assignment.slot)
            .collect::<HashSet<_>>();
        if evaluator.as_slice() != [evaluation.evaluator_player_id]
            || reviewers != evaluation.reviewer_player_ids
            || slots
                != HashSet::from([
                    SealedReviewAssignmentSlotV1::Evaluator,
                    SealedReviewAssignmentSlotV1::Reviewer1,
                    SealedReviewAssignmentSlotV1::Reviewer2,
                ])
        {
            return None;
        }
    }
    Some(())
}

fn sealed_reproductions(
    raw: &[Value],
    reproductions: &[SealedPaperReproductionV1],
    paper_id: Uuid,
    evaluations: &HashMap<Uuid, SealedEvaluationCrosslinkV1>,
    assignments: &[SealedReviewAssignmentV1],
) -> Option<()> {
    if raw.len() != reproductions.len() || reproductions.len() > 16_384 {
        return None;
    }
    let mut ids = HashSet::with_capacity(reproductions.len());
    let mut previous_id = None;
    let mut children = HashMap::with_capacity(reproductions.len());
    for (raw, reproduction) in raw.iter().zip(reproductions) {
        if raw.as_object()?.len() != 25
            || reproduction.schema != PAPER_REPRODUCTION_V1
            || reproduction.paper_project_id != paper_id
            || reproduction.version == 0
            || reproduction.signed_at_unix < 0
            || reproduction.observed_metrics_micros.len() > 256
            || reproduction.statistical_evidence.len() > 256
            || !sealed_uuid(raw, "reproduction_id", reproduction.reproduction_id)
            || !sealed_uuid(raw, "evaluation_id", reproduction.evaluation_id)
            || !sealed_uuid(raw, "paper_project_id", reproduction.paper_project_id)
            || !sealed_optional_uuid(
                raw,
                "supersedes_reproduction_id",
                reproduction.supersedes_reproduction_id,
            )
            || !sealed_uuid(
                raw,
                "reproducer_player_id",
                reproduction.reproducer_player_id,
            )
            || !sealed_timestamp(raw, "created_at", reproduction.created_at)
            || !ids.insert(reproduction.reproduction_id)
            || previous_id.is_some_and(|previous| previous >= reproduction.reproduction_id)
        {
            return None;
        }
        previous_id = Some(reproduction.reproduction_id);
        for digest in [
            &reproduction.release_candidate_hash,
            &reproduction.paper_bundle_hash,
            &reproduction.tolerance_policy_hash,
            &reproduction.seed_set_hash,
            &reproduction.environment_hash,
            &reproduction.run_manifest_hash,
            &reproduction.signing_public_key_hash,
            &reproduction.coi_attestation_hash,
            &reproduction.report_hash,
        ] {
            if !sealed_digest(digest) {
                return None;
            }
        }
        if !sealed_text(&reproduction.signing_key_id)
            || !sealed_public_key(
                &reproduction.signing_public_key,
                &reproduction.signing_public_key_hash,
            )
            || !sealed_canonical_base64(&reproduction.signature, 64)
            || reproduction
                .observed_metrics_micros
                .keys()
                .any(|key| !sealed_text(key))
            || reproduction
                .statistical_evidence
                .iter()
                .any(|(key, evidence)| {
                    !sealed_text(key)
                        || evidence.interval_overlap_bps > 10_000
                        || evidence.p_value_micros > 1_000_000
                })
        {
            return None;
        }
        let evaluation = evaluations.get(&reproduction.evaluation_id)?;
        if reproduction.release_candidate_hash != evaluation.release_candidate_hash
            || reproduction.paper_bundle_hash != evaluation.paper_bundle_hash
            || reproduction.tolerance_policy_hash != evaluation.tolerance_policy_hash
            || reproduction.created_at < evaluation.created_at
            || reproduction.reproducer_player_id == evaluation.evaluator_player_id
            || evaluation
                .reviewer_player_ids
                .contains(&reproduction.reproducer_player_id)
        {
            return None;
        }
        let result_keys = reproduction
            .rule_results
            .iter()
            .map(|result| {
                (sealed_text(&result.rule_key) && sealed_digest(&result.detail_hash))
                    .then_some(result.rule_key.clone())
            })
            .collect::<Option<Vec<_>>>()?;
        if result_keys != evaluation.tolerance_rule_keys
            || (reproduction.status == SealedReproductionStatusV1::Reproduced)
                != reproduction.rule_results.iter().all(|result| result.passed)
            || !assignments.iter().any(|assignment| {
                assignment.paper_project_id == paper_id
                    && assignment.submission_id == evaluation.submission_id
                    && assignment.review_round == evaluation.version
                    && assignment.slot == SealedReviewAssignmentSlotV1::Reproducer
                    && assignment.player_id == reproduction.reproducer_player_id
                    && assignment.claimed_at <= reproduction.created_at
                    && reproduction.created_at < assignment.expires_at
                    && assignment.pinned_evaluation_id.is_none()
                    && matches!(
                        assignment.status,
                        SealedReviewAssignmentStatusV1::Claimed
                            | SealedReviewAssignmentStatusV1::Expired
                    )
            })
        {
            return None;
        }
        let observed_hash =
            sha256_digest(&canonical_json_bytes(&reproduction.observed_metrics_micros).ok()?);
        let statistical_hash =
            sha256_digest(&canonical_json_bytes(&reproduction.statistical_evidence).ok()?);
        let signing = PaperReproductionSigningV1 {
            schema: PAPER_REPRODUCTION_V1.into(),
            reproduction_id: reproduction.reproduction_id,
            evaluation_id: reproduction.evaluation_id,
            paper_project_id: reproduction.paper_project_id,
            release_candidate_hash: reproduction.release_candidate_hash.clone(),
            paper_bundle_hash: reproduction.paper_bundle_hash.clone(),
            tolerance_policy_hash: reproduction.tolerance_policy_hash.clone(),
            observed_metrics_hash: observed_hash,
            statistical_evidence_hash: statistical_hash,
            seed_set_hash: reproduction.seed_set_hash.clone(),
            environment_hash: reproduction.environment_hash.clone(),
            run_manifest_hash: reproduction.run_manifest_hash.clone(),
            supersedes_reproduction_id: reproduction.supersedes_reproduction_id,
            reproducer_player_id: reproduction.reproducer_player_id,
            signing_key_id: reproduction.signing_key_id.clone(),
            signing_public_key_hash: reproduction.signing_public_key_hash.clone(),
            coi_attestation_hash: reproduction.coi_attestation_hash.clone(),
            signed_at_unix: reproduction.signed_at_unix,
        };
        let signing_hash = sha256_digest(&paper_reproduction_signing_bytes(&signing).ok()?);
        let expected_report_hash = sha256_digest(
            &canonical_json_bytes(&serde_json::json!({
                "signing_hash":signing_hash,
                "rule_results":&reproduction.rule_results,
                "status":reproduction.status,
            }))
            .ok()?,
        );
        if reproduction.report_hash != expected_report_hash {
            return None;
        }
        if let Some(parent) = reproduction.supersedes_reproduction_id {
            if children
                .insert(parent, reproduction.reproduction_id)
                .is_some()
            {
                return None;
            }
        }
    }
    let by_id = reproductions
        .iter()
        .map(|record| (record.reproduction_id, record))
        .collect::<HashMap<_, _>>();
    let mut roots = HashMap::<(Uuid, Uuid), usize>::new();
    for reproduction in reproductions {
        match reproduction.supersedes_reproduction_id {
            None if reproduction.version != 1 => return None,
            Some(parent_id) => {
                let parent = by_id.get(&parent_id)?;
                if parent.evaluation_id != reproduction.evaluation_id
                    || parent.reproducer_player_id != reproduction.reproducer_player_id
                    || parent.version.checked_add(1)? != reproduction.version
                    || parent.created_at > reproduction.created_at
                {
                    return None;
                }
            }
            None => {
                *roots
                    .entry((
                        reproduction.evaluation_id,
                        reproduction.reproducer_player_id,
                    ))
                    .or_default() += 1;
            }
        }
    }
    if roots.values().any(|count| *count != 1) {
        return None;
    }
    Some(())
}

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedPaperRoom {
    paper_id: Uuid,
    body: Value,
}

impl AuthenticatedPaperRoom {
    fn seal(route: &Route, paper_id: Uuid, body: Value) -> Result<Self, AppError> {
        let expected_path = format!("/v2/hepta/papers/{paper_id}/room");
        if route.method != Method::GET
            || route.path != expected_path
            || route.query.is_some()
            || route.operation != "get_paper_room_v3"
        {
            return Err(AppError::Internal);
        }
        let envelope: PaperRoomEnvelopeV3 =
            serde_json::from_value(body.clone()).map_err(|_| AppError::Upstream)?;
        let encoded_paper_id = envelope
            .paper
            .get("paper_project_id")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        if encoded_paper_id != paper_id.to_string() {
            return Err(AppError::Upstream);
        }
        envelope.consume_for_strict_shape();
        Ok(Self { paper_id, body })
    }

    pub(crate) fn paper_id(&self) -> Uuid {
        self.paper_id
    }

    pub(crate) fn value(&self) -> &Value {
        &self.body
    }

    #[cfg(test)]
    pub(crate) fn test_only_seal(paper_id: Uuid, body: Value) -> Result<Self, AppError> {
        Self::seal(
            &Route {
                method: Method::GET,
                path: format!("/v2/hepta/papers/{paper_id}/room"),
                query: None,
                operation: "get_paper_room_v3",
            },
            paper_id,
            body,
        )
    }
}

impl std::ops::Deref for AuthenticatedPaperRoom {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedPaperReviewState {
    paper_id: Uuid,
    body: Value,
}

impl AuthenticatedPaperReviewState {
    fn seal(route: &Route, paper_id: Uuid, body: Value) -> Result<Self, AppError> {
        let expected_path = format!("/v2/hepta/papers/{paper_id}/review-state");
        if route.method != Method::GET
            || route.path != expected_path
            || route.query.is_some()
            || route.operation != "get_paper_review_state_v1"
        {
            return Err(AppError::Internal);
        }
        let envelope: PaperReviewStateEnvelopeV1 =
            serde_json::from_value(body.clone()).map_err(|_| AppError::Upstream)?;
        let finality = &envelope.finality;
        let raw_finality = body.get("finality").ok_or(AppError::Upstream)?;
        let encoded_verified_at = body
            .get("finality")
            .and_then(|value| value.get("verified_at"));
        let canonical_finality_timestamp = match (&finality.status, &finality.verified_at) {
            (SealedConsumerFinalityStatusV2::PendingFinality, None) => {
                encoded_verified_at.is_some_and(Value::is_null)
            }
            (SealedConsumerFinalityStatusV2::VerifiedFinality, Some(verified_at)) => {
                encoded_verified_at
                    .and_then(Value::as_str)
                    .is_some_and(|encoded| {
                        encoded == verified_at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
                    })
            }
            _ => false,
        };
        let canonical_finality_bindings = sealed_optional_uuid(
            raw_finality,
            "effective_evaluation_id",
            finality.effective_evaluation_id,
        ) && sealed_optional_uuid(
            raw_finality,
            "effective_reproduction_id",
            finality.effective_reproduction_id,
        ) && sealed_optional_uuid(
            raw_finality,
            "effective_appeal_resolution_id",
            finality.effective_appeal_resolution_id,
        );
        let finality_bindings_valid = match finality.status {
            SealedConsumerFinalityStatusV2::PendingFinality => {
                finality.effective_evaluation_id.is_none()
                    && finality.effective_reproduction_id.is_none()
                    && finality.effective_appeal_resolution_id.is_none()
            }
            SealedConsumerFinalityStatusV2::VerifiedFinality => {
                finality.effective_evaluation_id.is_some()
                    && finality.effective_reproduction_id.is_some()
            }
        };
        if finality.schema != "hepta.paper_raid.consumer_finality.v2"
            || finality.ranking_eligible
            || finality.reward_eligible
            || finality.score_eligible
            || finality.economic_eligible
            || !canonical_finality_bindings
            || !finality_bindings_valid
            || !canonical_finality_timestamp
        {
            return Err(AppError::Upstream);
        }
        let expected_paper_id = paper_id.to_string();
        for records in [
            &envelope.contribution_ledgers,
            &envelope.evaluations,
            &envelope.appeals,
            &envelope.resolutions,
            &envelope.raid_scores,
        ] {
            for record in records {
                if record.get("paper_project_id").and_then(Value::as_str)
                    != Some(expected_paper_id.as_str())
                {
                    return Err(AppError::Upstream);
                }
            }
        }
        let raw_assignments = body
            .get("assignments")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let raw_evaluation_drafts = body
            .get("evaluation_drafts")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let raw_reproductions = body
            .get("reproductions")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let evaluations = sealed_evaluation_crosslinks(&envelope.evaluations, paper_id)
            .ok_or(AppError::Upstream)?;
        let evaluation_drafts = sealed_evaluation_draft_crosslinks(
            raw_evaluation_drafts,
            &envelope.evaluation_drafts,
            paper_id,
            &evaluations,
        )
        .ok_or(AppError::Upstream)?;
        sealed_review_assignments(
            raw_assignments,
            &envelope.assignments,
            paper_id,
            &evaluations,
            &evaluation_drafts,
        )
        .ok_or(AppError::Upstream)?;
        sealed_reproductions(
            raw_reproductions,
            &envelope.reproductions,
            paper_id,
            &evaluations,
            &envelope.assignments,
        )
        .ok_or(AppError::Upstream)?;
        Ok(Self { paper_id, body })
    }

    pub(crate) fn paper_id(&self) -> Uuid {
        self.paper_id
    }

    pub(crate) fn value(&self) -> &Value {
        &self.body
    }

    pub(crate) fn finality(&self) -> &Value {
        self.body
            .get("finality")
            .expect("sealed review state contains finality")
    }

    #[cfg(test)]
    pub(crate) fn test_only_seal(paper_id: Uuid, body: Value) -> Result<Self, AppError> {
        Self::seal(
            &Route {
                method: Method::GET,
                path: format!("/v2/hepta/papers/{paper_id}/review-state"),
                query: None,
                operation: "get_paper_review_state_v1",
            },
            paper_id,
            body,
        )
    }
}

impl std::ops::Deref for AuthenticatedPaperReviewState {
    type Target = Value;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl BrowserCommand {
    fn route(&self) -> Result<Route, AppError> {
        let resource = || {
            self.resource_id
                .ok_or_else(|| AppError::Invalid("resource_id is required".into()))
        };
        let child = || {
            self.child_id
                .ok_or_else(|| AppError::Invalid("child_id is required".into()))
        };
        let post = |path, operation| Route {
            method: Method::POST,
            path,
            query: None,
            operation,
        };
        Ok(match self.command {
            CommandName::RotateHumanSigningKey => post(
                format!("/v2/hepta/players/{}/signing-key/rotate", resource()?),
                "rotate_human_signing_key_v2",
            ),
            CommandName::RevokeHumanSigningKey => post(
                format!("/v2/hepta/players/{}/signing-key/revoke", resource()?),
                "revoke_human_signing_key_v2",
            ),
            CommandName::CreateAgentBinding => {
                post("/v2/hepta/agent-bindings".into(), "create_agent_binding_v2")
            }
            CommandName::RotateAgentBindingKey => post(
                format!("/v2/hepta/agent-bindings/{}/rotate-key", resource()?),
                "rotate_agent_binding_key_v2",
            ),
            CommandName::CreateResearchTeam => {
                post("/v2/hepta/teams".into(), "create_research_team_v2")
            }
            CommandName::AcceptResearchTeamMembership => post(
                format!("/v2/hepta/teams/{}/member-acceptances", resource()?),
                "accept_research_team_membership_v2",
            ),
            CommandName::LockResearchTeam => post(
                format!("/v2/hepta/teams/{}/lock", resource()?),
                "lock_research_team_v2",
            ),
            CommandName::CreatePaperProject => {
                post("/v2/hepta/papers".into(), "create_paper_project_v2")
            }
            CommandName::TransitionPaperChallengeOutcome => post(
                format!("/v2/hepta/papers/{}/outcome", resource()?),
                "transition_paper_challenge_outcome_v1",
            ),
            CommandName::TransitionPaperProject => post(
                format!("/v2/hepta/papers/{}/transition", resource()?),
                "transition_paper_project_v2",
            ),
            CommandName::CreatePaperWorkItem => post(
                format!("/v2/hepta/papers/{}/work-items", resource()?),
                "create_paper_work_item_v2",
            ),
            CommandName::TransitionPaperWorkItem => post(
                format!("/v2/hepta/work-items/{}/transition", resource()?),
                "transition_paper_work_item_v2",
            ),
            CommandName::CreatePaperRevision => post(
                format!("/v2/hepta/papers/{}/revisions", resource()?),
                "create_paper_revision_v2",
            ),
            CommandName::PromotePaperReleaseCandidate => post(
                format!(
                    "/v2/hepta/papers/{}/revisions/{}/promote",
                    resource()?,
                    child()?
                ),
                "promote_paper_release_candidate_v2",
            ),
            CommandName::CreateAuthorshipConsent => post(
                format!("/v2/hepta/papers/{}/author-consents", resource()?),
                "create_authorship_consent_v2",
            ),
            CommandName::FinalizeJointPaperSubmission => post(
                format!("/v2/hepta/papers/{}/finalize", resource()?),
                "finalize_joint_paper_submission_v2",
            ),
            CommandName::StartPaperRework => post(
                format!("/v2/hepta/papers/{}/reworks", resource()?),
                "start_paper_rework_v1",
            ),
            CommandName::IssueResearchSessionAuthorizationSet => post(
                "/v2/hepta/research-session-authorizations".into(),
                "issue_research_session_authorization_set_v1",
            ),
            CommandName::ReplaceResearchSessionAuthorizationSet => {
                let session_id = self
                    .session_id
                    .as_deref()
                    .ok_or_else(|| AppError::Invalid("session_id is required".into()))?;
                validate_logical_id(session_id)?;
                post(
                    format!("/v2/hepta/research-session-authorizations/{session_id}/replace"),
                    "replace_research_session_authorization_set_v1",
                )
            }
            CommandName::CreateNakamaResearchSessionControl => {
                self.require_no_route_locators()?;
                post(
                    "/v2/hepta/nakama/research-session-controls/create".into(),
                    "create_nakama_research_session_control_v2",
                )
            }
            CommandName::ResumeNakamaResearchSessionControl => {
                self.require_no_route_locators()?;
                post(
                    "/v2/hepta/nakama/research-session-controls/resume".into(),
                    "resume_nakama_research_session_control_v2",
                )
            }
            CommandName::ReplaceNakamaResearchSessionRosterControl => {
                self.require_no_route_locators()?;
                post(
                    "/v2/hepta/nakama/research-session-controls/replace-roster".into(),
                    "replace_nakama_research_session_roster_control_v2",
                )
            }
            CommandName::CompleteNakamaResearchSessionControl => {
                self.require_no_route_locators()?;
                post(
                    "/v2/hepta/nakama/research-session-controls/complete".into(),
                    "complete_nakama_research_session_control_v2",
                )
            }
            CommandName::QueueMatchmaking => post(
                "/v2/hepta/matchmaking/tickets".into(),
                "create_matchmaking_ticket_v3",
            ),
            CommandName::CancelMatchmakingTicket => post(
                format!("/v2/hepta/matchmaking/tickets/{}/cancel", resource()?),
                "cancel_matchmaking_ticket_v3",
            ),
            CommandName::DecideTeamProposal => post(
                format!("/v2/hepta/team-proposals/{}/decisions", resource()?),
                "create_team_proposal_decision_v3",
            ),
            CommandName::MaterializeTeamProposal => post(
                format!("/v2/hepta/team-proposals/{}/materialize", resource()?),
                "materialize_team_proposal_v1",
            ),
            CommandName::RegisterArtifact => post(
                format!("/v2/hepta/papers/{}/artifact-manifests", resource()?),
                "create_artifact_manifest_v3",
            ),
            CommandName::CreateEvidenceCard => post(
                format!("/v2/hepta/papers/{}/evidence-cards", resource()?),
                "create_evidence_card_v3",
            ),
            CommandName::CreateCitationRecord => post(
                format!("/v2/hepta/papers/{}/citations", resource()?),
                "create_citation_record_v3",
            ),
            CommandName::CreateExperimentPlan => post(
                format!("/v2/hepta/papers/{}/experiment-plans", resource()?),
                "create_experiment_plan_v3",
            ),
            CommandName::CreateRunRecord => post(
                format!("/v2/hepta/papers/{}/run-records", resource()?),
                "create_run_record_v3",
            ),
            CommandName::CreateRoleResourceAction => post(
                format!("/v2/hepta/papers/{}/role-resources/actions", resource()?),
                "create_role_resource_action_v1",
            ),
            CommandName::CreateFigureLineage => post(
                format!("/v2/hepta/papers/{}/figure-lineage", resource()?),
                "create_figure_lineage_v3",
            ),
            CommandName::CreateClaimRecord => post(
                format!("/v2/hepta/papers/{}/claims", resource()?),
                "create_claim_record_v3",
            ),
            CommandName::AcquireSectionLease => post(
                format!("/v2/hepta/papers/{}/section-leases", resource()?),
                "acquire_section_lease_v3",
            ),
            CommandName::SubmitAgentProposal => post(
                format!("/v2/hepta/papers/{}/agent-proposals", resource()?),
                "create_agent_proposal_v3",
            ),
            CommandName::RecordHumanDecision => post(
                format!("/v2/hepta/papers/{}/human-decisions", resource()?),
                "create_human_decision_v3",
            ),
            CommandName::CreateSectionRevision => post(
                format!("/v2/hepta/papers/{}/section-revisions", resource()?),
                "create_section_revision_v3",
            ),
            CommandName::SubmitReview => post(
                format!(
                    "/v2/hepta/papers/{}/section-revisions/{}/reviews",
                    resource()?,
                    child()?
                ),
                "create_section_review_v3",
            ),
            CommandName::MergeSection => post(
                format!("/v2/hepta/papers/{}/section-merges", resource()?),
                "create_section_merge_v3",
            ),
            CommandName::CreateContributionLedger => post(
                format!("/v2/hepta/papers/{}/contribution-ledgers", resource()?),
                "create_contribution_ledger_v1",
            ),
            CommandName::ClaimReviewAssignment => post(
                format!("/v2/hepta/papers/{}/review-assignments", resource()?),
                "claim_paper_review_assignment_v1",
            ),
            CommandName::CreatePaperEvaluation => post(
                format!("/v2/hepta/papers/{}/evaluations", resource()?),
                "create_paper_evaluation_v1",
            ),
            CommandName::CreatePaperEvaluationDraft => {
                let paper_id = self.require_resource_only()?;
                post(
                    format!("/v2/hepta/papers/{paper_id}/evaluation-drafts"),
                    "create_paper_evaluation_draft_v1",
                )
            }
            CommandName::SubmitEvaluationDraftAttestation => {
                let (paper_id, evaluation_id) = self.require_resource_child()?;
                post(
                    format!(
                        "/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/attestations"
                    ),
                    "submit_paper_evaluation_draft_attestation_v1",
                )
            }
            CommandName::FinalizePaperEvaluationDraft => {
                let (paper_id, evaluation_id) = self.require_resource_child()?;
                post(
                    format!(
                        "/v2/hepta/papers/{paper_id}/evaluation-drafts/{evaluation_id}/finalize"
                    ),
                    "finalize_paper_evaluation_draft_v1",
                )
            }
            CommandName::SubmitReproduction => {
                let (paper_id, evaluation_id) = self.require_resource_child()?;
                post(
                    format!(
                        "/v2/hepta/papers/{paper_id}/evaluations/{evaluation_id}/reproductions"
                    ),
                    "create_paper_reproduction_v1",
                )
            }
            CommandName::SubmitAppeal => post(
                format!(
                    "/v2/hepta/papers/{}/evaluations/{}/appeals",
                    resource()?,
                    child()?
                ),
                "create_paper_appeal_v1",
            ),
            CommandName::ResolveAppeal => post(
                format!(
                    "/v2/hepta/papers/{}/appeals/{}/resolve",
                    resource()?,
                    child()?
                ),
                "resolve_paper_appeal_v1",
            ),
        })
    }

    fn canonical_payload(&self) -> Result<Value, AppError> {
        let mut payload = self.payload.clone();
        let object = payload
            .as_object_mut()
            .ok_or_else(|| AppError::Invalid("command payload must be a JSON object".into()))?;
        let expected = self.idempotency_key.to_string();
        match object.get("idempotency_key") {
            Some(Value::String(value)) if value == &expected => {}
            Some(_) => {
                return Err(AppError::Conflict(
                    "payload idempotency_key does not match command envelope".into(),
                ))
            }
            None => {
                object.insert("idempotency_key".into(), Value::String(expected));
            }
        }
        Ok(payload)
    }

    fn require_no_route_locators(&self) -> Result<(), AppError> {
        if self.resource_id.is_some() || self.child_id.is_some() || self.session_id.is_some() {
            return Err(AppError::Invalid(
                "Nakama research control commands require null envelope locators".into(),
            ));
        }
        Ok(())
    }

    fn require_resource_only(&self) -> Result<Uuid, AppError> {
        if self.child_id.is_some() || self.session_id.is_some() {
            return Err(AppError::Invalid(
                "command requires exactly one resource locator".into(),
            ));
        }
        self.resource_id
            .ok_or_else(|| AppError::Invalid("resource_id is required".into()))
    }

    fn require_resource_child(&self) -> Result<(Uuid, Uuid), AppError> {
        if self.session_id.is_some() {
            return Err(AppError::Invalid(
                "command does not accept a session locator".into(),
            ));
        }
        Ok((
            self.resource_id
                .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?,
            self.child_id
                .ok_or_else(|| AppError::Invalid("child_id is required".into()))?,
        ))
    }
}

#[derive(Debug)]
pub struct UpstreamResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub replayed: bool,
}

impl UpstreamResponse {
    pub fn json(&self) -> Result<Value, AppError> {
        serde_json::from_slice(&self.body).map_err(|_| AppError::Upstream)
    }
}

#[derive(Clone)]
pub struct HeptaClient {
    client: Client,
    base: Url,
    assertions: ConsumerAssertionConfig,
    pool: PgPool,
    metrics: Metrics,
}

impl HeptaClient {
    pub fn new(
        base: Url,
        assertions: ConsumerAssertionConfig,
        pool: PgPool,
        metrics: Metrics,
    ) -> Result<Self, String> {
        if base.path() != "/" && !base.path().is_empty() {
            return Err("Hepta base URL must not contain a path".to_string());
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(8))
            .user_agent("paper-raid-bff/0.1")
            .build()
            .map_err(|error| format!("cannot build Hepta client: {error}"))?;
        Ok(Self {
            client,
            base,
            assertions,
            pool,
            metrics,
        })
    }

    pub async fn ready(&self) -> bool {
        let Ok(url) = join_exact(&self.base, "/ready") else {
            return false;
        };
        let Ok(response) = self.client.get(url).send().await else {
            return false;
        };
        if response.status() != StatusCode::OK || !strict_json(response.headers()) {
            return false;
        }
        limited_body(response, 64 * 1024).await.is_ok()
    }

    pub async fn get_current_human_player(
        &self,
        identity: &AlphaIdentity,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/players/me".into(),
            None,
            "get_self_human_player_v2",
        )
        .await
    }

    pub async fn list_current_agent_bindings(
        &self,
        identity: &AlphaIdentity,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/agent-bindings".into(),
            None,
            "list_self_agent_bindings_v2",
        )
        .await
    }

    pub async fn create_agent_binding_v3(
        &self,
        identity: &AlphaIdentity,
        idempotency_key: Uuid,
        payload: &Value,
    ) -> Result<UpstreamResponse, AppError> {
        self.send(
            identity,
            Route {
                method: Method::POST,
                path: "/v2/hepta/agent-bindings".into(),
                query: None,
                operation: "create_agent_binding_v3",
            },
            idempotency_key,
            payload,
        )
        .await
    }

    pub fn human_registration_challenge(
        identity: &AlphaIdentity,
        signing_public_key: &str,
        now_unix: i64,
    ) -> Result<HumanRegistrationChallenge, AppError> {
        if now_unix < 0 {
            return Err(AppError::Internal);
        }
        let issued_at_unix = now_unix.div_euclid(HUMAN_REGISTRATION_BUCKET_SECONDS)
            * HUMAN_REGISTRATION_BUCKET_SECONDS;
        build_human_registration_challenge(identity, signing_public_key, issued_at_unix, now_unix)
    }

    pub async fn create_human_player(
        &self,
        identity: &AlphaIdentity,
        proof: &HumanRegistrationProof,
    ) -> Result<UpstreamResponse, AppError> {
        let challenge = registration_challenge_from_proof(identity, proof, Utc::now().timestamp())?;
        let payload = serde_json::json!({
            "player_id": identity.player_id,
            "display_name": identity.display_name,
            "signing_key_id": challenge.signing_key_id,
            "signing_public_key": challenge.signing_public_key,
            "key_issued_at_unix": challenge.issued_at_unix,
            "key_expires_at_unix": challenge.expires_at_unix,
            "key_proof_signature": proof.key_proof_signature,
            "idempotency_key": challenge.idempotency_key,
        });
        self.send(
            identity,
            Route {
                method: Method::POST,
                path: "/v2/hepta/players".into(),
                query: None,
                operation: "create_human_player_v2",
            },
            challenge.idempotency_key,
            &payload,
        )
        .await
    }

    pub async fn recover_human_registration(
        &self,
        identity: &AlphaIdentity,
        proof: &HumanRegistrationProof,
    ) -> Result<Option<Value>, AppError> {
        let challenge = registration_challenge_from_proof(identity, proof, proof.issued_at_unix)?;
        let value = match self.get_current_human_player(identity).await {
            Ok(value) => value,
            Err(AppError::NotFound) => return Ok(None),
            Err(error) => return Err(error),
        };
        validate_recovered_human_player(identity, &challenge, &value)?;
        Ok(Some(value))
    }

    pub async fn human_signing_frame(
        &self,
        identity: &AlphaIdentity,
        request: &HumanSigningFrameRequest,
    ) -> Result<HumanSigningFrameResponse, AppError> {
        let player: PlayerSnapshot =
            serde_json::from_value(self.get_current_human_player(identity).await?)
                .map_err(|_| AppError::Upstream)?;
        if player.player_id != identity.player_id || player.subject_id != identity.subject_id {
            return Err(AppError::Forbidden);
        }
        let signed_at_unix = Utc::now().timestamp();
        let derived_resource_id = request.resource_id;
        let mut derived_child_id = request.child_id;
        let (payload, signing_bytes) = match request.command {
            CommandName::AcceptResearchTeamMembership => {
                let team_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: AcceptanceFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| {
                        AppError::Invalid("invalid team acceptance frame payload".into())
                    })?;
                let team: TeamSigningSnapshot =
                    serde_json::from_value(self.get_team(identity, team_id).await?)
                        .map_err(|_| AppError::Upstream)?;
                let member = team
                    .members
                    .iter()
                    .find(|member| member.player_id == identity.player_id)
                    .ok_or(AppError::Forbidden)?;
                if team.team_id != team_id
                    || team.version != input.expected_team_version
                    || team.roster_version != input.roster_version
                    || member.participant_slot != input.participant_slot
                    || member.binding_id != input.binding_id
                    || member.role != input.role
                    || team.collaboration_compact_hash != input.collaboration_compact_hash
                {
                    return Err(AppError::Conflict(
                        "team acceptance payload does not match the current roster".into(),
                    ));
                }
                let signing = TeamMemberAcceptanceSigningV2 {
                    schema: TEAM_MEMBER_ACCEPTANCE_V2.into(),
                    acceptance_id: input.acceptance_id,
                    team_id,
                    challenge_id: team.challenge_id,
                    roster_version: team.roster_version,
                    participant_slot: member.participant_slot,
                    player_id: identity.player_id,
                    binding_id: member.binding_id,
                    agent_id: member.agent_id.clone(),
                    role: member.role.clone(),
                    collaboration_compact_hash: team.collaboration_compact_hash.clone(),
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key: player.signing_public_key.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    accepted_at_unix: signed_at_unix,
                };
                let bytes =
                    team_member_acceptance_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "acceptance_id": input.acceptance_id,
                    "expected_team_version": input.expected_team_version,
                    "roster_version": input.roster_version,
                    "participant_slot": input.participant_slot,
                    "binding_id": input.binding_id,
                    "role": input.role,
                    "collaboration_compact_hash": input.collaboration_compact_hash,
                    "accepted_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::CreateEvidenceCard => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: EvidenceVerificationFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid evidence verification frame payload".into())
                    })?;
                validate_frame_https_uri(&input.source_uri)?;
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let signing = HumanEvidenceVerificationSigningV1 {
                    schema: HUMAN_EVIDENCE_VERIFICATION_V1.into(),
                    verification_id: input.evidence_card_id,
                    paper_project_id: paper_id,
                    record_kind: "evidence_card".into(),
                    record_id: input.evidence_card_id,
                    source_identifier: format!("evidence-uri\n{}", input.source_uri),
                    source_hash: input.source_hash.clone(),
                    locator: input.locator.clone(),
                    license: input.license.clone(),
                    player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = human_evidence_verification_signing_bytes(&signing)
                    .map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "evidence_card_id": input.evidence_card_id,
                    "source_uri": input.source_uri,
                    "source_hash": input.source_hash,
                    "locator": input.locator,
                    "license": input.license,
                    "verification_key_id": player.signing_key_id.clone(),
                    "verification_public_key": player.signing_public_key.clone(),
                    "verification_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::CreateCitationRecord => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: CitationVerificationFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid citation verification frame payload".into())
                    })?;
                if input.doi.is_none() && input.canonical_url.is_none() {
                    return Err(AppError::Invalid(
                        "citation requires a DOI or canonical_url".into(),
                    ));
                }
                if let Some(doi) = input.doi.as_deref() {
                    if !doi.starts_with("10.")
                        || !doi.contains('/')
                        || doi.bytes().any(|byte| byte.is_ascii_whitespace())
                    {
                        return Err(AppError::Invalid("citation DOI is invalid".into()));
                    }
                }
                if let Some(url) = input.canonical_url.as_deref() {
                    validate_frame_https_uri(url)?;
                }
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let evidence_id = input.evidence_card_id.to_string();
                let evidence = room
                    .get("evidence_cards")
                    .and_then(Value::as_array)
                    .and_then(|records| {
                        records.iter().find(|record| {
                            record.get("evidence_card_id").and_then(Value::as_str)
                                == Some(evidence_id.as_str())
                        })
                    })
                    .ok_or(AppError::NotFound)?;
                let source_hash = required_string(evidence, "source_hash")?;
                let locator = required_string(evidence, "locator")?;
                let license = required_string(evidence, "license")?;
                let source_identifier = format!(
                    "citation-doi\n{}\ncitation-url\n{}",
                    input.doi.as_deref().unwrap_or("-"),
                    input.canonical_url.as_deref().unwrap_or("-")
                );
                let signing = HumanEvidenceVerificationSigningV1 {
                    schema: HUMAN_EVIDENCE_VERIFICATION_V1.into(),
                    verification_id: input.citation_id,
                    paper_project_id: paper_id,
                    record_kind: "citation".into(),
                    record_id: input.citation_id,
                    source_identifier,
                    source_hash: source_hash.clone(),
                    locator: locator.clone(),
                    license: license.clone(),
                    player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = human_evidence_verification_signing_bytes(&signing)
                    .map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "citation_id": input.citation_id,
                    "evidence_card_id": input.evidence_card_id,
                    "doi": input.doi,
                    "canonical_url": input.canonical_url,
                    "source_hash": source_hash,
                    "locator": locator,
                    "license": license,
                    "verification_key_id": player.signing_key_id.clone(),
                    "verification_public_key": player.signing_public_key.clone(),
                    "verification_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::RecordHumanDecision => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: HumanDecisionFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid human decision frame payload".into())
                    })?;
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let proposal =
                    find_room_record(room, "proposals", "proposal_id", input.proposal_id)?;
                if required_u64(proposal, "version")? != input.expected_proposal_version
                    || proposal.get("status").and_then(Value::as_str) != Some("submitted")
                {
                    return Err(AppError::Conflict(
                        "human decision must bind the current submitted proposal".into(),
                    ));
                }
                let signing = HumanDecisionSigningV1 {
                    schema: HUMAN_DECISION_V1.into(),
                    decision_id: input.decision_id,
                    paper_project_id: paper_id,
                    proposal_id: input.proposal_id,
                    player_id: identity.player_id,
                    decision: input.decision.clone(),
                    reason_hash: input.reason_hash.clone(),
                    expected_proposal_version: input.expected_proposal_version,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = human_decision_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "decision_id": input.decision_id,
                    "proposal_id": input.proposal_id,
                    "expected_proposal_version": input.expected_proposal_version,
                    "decision": input.decision,
                    "reason_hash": input.reason_hash,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::SubmitReview => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let section_revision_id = request
                    .child_id
                    .ok_or_else(|| AppError::Invalid("child_id is required".into()))?;
                let input: ReviewFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| AppError::Invalid("invalid review frame payload".into()))?;
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let revision = find_room_record(
                    room,
                    "section_revisions",
                    "section_revision_id",
                    section_revision_id,
                )?;
                if required_u64(revision, "version")? != input.expected_revision_version
                    || revision.get("status").and_then(Value::as_str) != Some("proposed")
                {
                    return Err(AppError::Conflict(
                        "section review must bind the current proposed revision".into(),
                    ));
                }
                let lease_id = required_uuid(revision, "lease_id")?;
                let lease = find_room_record(room, "leases", "lease_id", lease_id)?;
                if required_uuid(lease, "holder_player_id")? == identity.player_id {
                    return Err(AppError::Forbidden);
                }
                let signing = SectionReviewSigningV1 {
                    schema: SECTION_REVIEW_V1.into(),
                    review_id: input.review_id,
                    paper_project_id: paper_id,
                    section_revision_id,
                    reviewer_player_id: identity.player_id,
                    verdict: input.verdict.clone(),
                    review_hash: input.review_hash.clone(),
                    expected_revision_version: input.expected_revision_version,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = section_review_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "review_id": input.review_id,
                    "expected_revision_version": input.expected_revision_version,
                    "verdict": input.verdict,
                    "review_hash": input.review_hash,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::MergeSection => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: SectionMergeFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid section merge frame payload".into())
                    })?;
                if input.merged_section_revision_id != input.section_revision_id {
                    return Err(AppError::Invalid(
                        "merge must advance to the reviewed section revision".into(),
                    ));
                }
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let revision = find_room_record(
                    room,
                    "section_revisions",
                    "section_revision_id",
                    input.section_revision_id,
                )?;
                let section_key = required_string(revision, "section_key")?;
                if required_u64(revision, "version")? != input.expected_revision_version
                    || revision.get("status").and_then(Value::as_str) != Some("approved")
                    || required_uuid(revision, "parent_revision_id")? != input.parent_revision_id
                    || required_uuid(revision, "lease_id")? != input.lease_id
                    || required_u64(revision, "fencing_token")? != input.fencing_token
                {
                    return Err(AppError::Conflict(
                        "section merge must bind the current approved revision".into(),
                    ));
                }
                let lease = find_room_record(room, "leases", "lease_id", input.lease_id)?;
                let lease_expires_at = lease
                    .get("expires_at")
                    .and_then(Value::as_str)
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .ok_or(AppError::Upstream)?;
                if required_uuid(lease, "holder_player_id")? != identity.player_id
                    || lease.get("status").and_then(Value::as_str) != Some("active")
                    || lease.get("section_key").and_then(Value::as_str)
                        != Some(section_key.as_str())
                    || required_u64(lease, "fencing_token")? != input.fencing_token
                    || lease_expires_at.timestamp() <= signed_at_unix
                {
                    return Err(AppError::Conflict(
                        "section merge requires the holder's current unexpired lease".into(),
                    ));
                }
                let head = room
                    .get("section_heads")
                    .and_then(Value::as_array)
                    .and_then(|records| {
                        records.iter().find(|record| {
                            record.get("section_key").and_then(Value::as_str)
                                == Some(section_key.as_str())
                        })
                    })
                    .ok_or(AppError::NotFound)?;
                if required_uuid(head, "current_head_revision_id")? != input.parent_revision_id
                    || required_u64(head, "fencing_token")? != input.fencing_token
                {
                    return Err(AppError::Conflict(
                        "section merge does not match the authoritative head".into(),
                    ));
                }
                let signing = SectionMergeSigningV1 {
                    schema: SECTION_MERGE_V1.into(),
                    merge_id: input.merge_id,
                    paper_project_id: paper_id,
                    section_key,
                    section_revision_id: input.section_revision_id,
                    parent_revision_id: input.parent_revision_id,
                    merged_section_revision_id: input.merged_section_revision_id,
                    lease_id: input.lease_id,
                    fencing_token: input.fencing_token,
                    merged_by_player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    merged_at_unix: signed_at_unix,
                };
                let bytes = section_merge_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "merge_id": input.merge_id,
                    "section_revision_id": input.section_revision_id,
                    "expected_revision_version": input.expected_revision_version,
                    "parent_revision_id": input.parent_revision_id,
                    "merged_section_revision_id": input.merged_section_revision_id,
                    "lease_id": input.lease_id,
                    "fencing_token": input.fencing_token,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "merged_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::CreateAuthorshipConsent => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: ConsentFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| AppError::Invalid("invalid authorship frame payload".into()))?;
                let signing = AuthorshipConsentSigningV2 {
                    schema: AUTHORSHIP_CONSENT_V2.into(),
                    consent_id: input.consent_id,
                    paper_project_id: paper_id,
                    revision_id: input.revision_id,
                    player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key: player.signing_public_key.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    release_candidate_hash: input.release_candidate_hash.clone(),
                    signed_at_unix,
                };
                let bytes =
                    authorship_consent_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "consent_id": input.consent_id,
                    "expected_paper_version": input.expected_paper_version,
                    "revision_id": input.revision_id,
                    "player_id": identity.player_id,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "release_candidate_hash": input.release_candidate_hash,
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::CreatePaperEvaluationDraft => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: EvaluationDraftFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid evaluation draft frame payload".into())
                    })?;
                let bundle = self.get_paper_review_bundle(identity, paper_id).await?;
                let (submission_id, release_candidate_hash, paper_bundle_hash) =
                    require_review_bundle_scope(&bundle, paper_id, identity)?;
                if bundle
                    .get("evaluation_quorum")
                    .is_some_and(|value| !value.is_null())
                    || bundle
                        .get("evaluation")
                        .is_some_and(|value| !value.is_null())
                {
                    return Err(AppError::Conflict(
                        "the assigned review round already has an authoritative draft or evaluation"
                            .into(),
                    ));
                }
                let player_id = identity.player_id.to_string();
                let assignment = bundle
                    .get("my_assignments")
                    .and_then(Value::as_array)
                    .and_then(|assignments| {
                        assignments.iter().find(|assignment| {
                            assignment.get("slot").and_then(Value::as_str) == Some("evaluator")
                                && assignment.get("player_id").and_then(Value::as_str)
                                    == Some(player_id.as_str())
                        })
                    })
                    .ok_or(AppError::Forbidden)?;
                let review_round = required_u64(assignment, "review_round")?;
                require_exact_review_assignment(
                    &bundle,
                    identity,
                    submission_id,
                    review_round,
                    &["evaluator"],
                )?;
                if input.reference_metrics_micros.is_empty()
                    || input.reference_metrics_micros.len() > 256
                    || input.tolerance_policy.get("schema").and_then(Value::as_str)
                        != Some("hepta.paper_raid.tolerance_policy.v1")
                    || input
                        .tolerance_policy
                        .get("version")
                        .and_then(Value::as_str)
                        != Some("1")
                    || input
                        .tolerance_policy
                        .get("rules")
                        .and_then(Value::as_array)
                        .is_none_or(Vec::is_empty)
                {
                    return Err(AppError::Invalid(
                        "evaluation requires a frozen v1 tolerance policy and reference metrics"
                            .into(),
                    ));
                }
                let score_bps = input.score_components.checked_total()?;
                let eligible = input.hard_gates.eligible();
                let score_components = serde_json::to_value(&input.score_components)
                    .map_err(|_| AppError::Internal)?;
                let hard_gates =
                    serde_json::to_value(&input.hard_gates).map_err(|_| AppError::Internal)?;
                let paper_score_hash = canonical_value_hash(&serde_json::json!({
                    "schema": "hepta.paper_raid.paper_score.v1",
                    "evaluation_id": input.evaluation_id,
                    "paper_project_id": paper_id,
                    "components": score_components,
                    "hard_gates": hard_gates,
                    "score_bps": score_bps,
                    "eligible": eligible,
                }))?;
                let tolerance_policy_hash = canonical_value_hash(&input.tolerance_policy)?;
                let reference_metrics_hash = canonical_value_hash(&input.reference_metrics_micros)?;
                let hard_gates_hash = canonical_value_hash(&input.hard_gates)?;
                let signing = PaperEvaluationSigningV1 {
                    schema: PAPER_EVALUATION_V1.into(),
                    evaluation_id: input.evaluation_id,
                    paper_project_id: paper_id,
                    submission_id,
                    release_candidate_hash: release_candidate_hash.clone(),
                    paper_bundle_hash: paper_bundle_hash.clone(),
                    supersedes_evaluation_id: input.supersedes_evaluation_id,
                    tolerance_policy_hash,
                    paper_score_hash,
                    reference_metrics_hash,
                    hard_gates_hash,
                    evaluator_player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    coi_attestation_hash: input.evaluator_coi_attestation_hash.clone(),
                    signed_at_unix,
                };
                let bytes = paper_evaluation_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "evaluation_id": input.evaluation_id,
                    "submission_id": submission_id,
                    "supersedes_evaluation_id": input.supersedes_evaluation_id,
                    "release_candidate_hash": release_candidate_hash,
                    "paper_bundle_hash": paper_bundle_hash,
                    "tolerance_policy": input.tolerance_policy,
                    "reference_metrics_micros": input.reference_metrics_micros,
                    "score_components": input.score_components,
                    "hard_gates": input.hard_gates,
                    "evaluator_player_id": identity.player_id,
                    "evaluator_signing_key_id": player.signing_key_id.clone(),
                    "evaluator_signing_public_key": player.signing_public_key.clone(),
                    "evaluator_signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "evaluator_coi_attestation_hash": input.evaluator_coi_attestation_hash,
                    "evaluator_signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::SubmitEvaluationDraftAttestation => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let evaluation_id = request
                    .child_id
                    .ok_or_else(|| AppError::Invalid("child_id is required".into()))?;
                let input: EvaluationAttestationFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid evaluation attestation frame payload".into())
                    })?;
                if !matches!(input.verdict.as_str(), "approve" | "reject") {
                    return Err(AppError::Invalid(
                        "review verdict must be approve or reject".into(),
                    ));
                }
                let bundle = self.get_paper_review_bundle(identity, paper_id).await?;
                let (submission_id, _, _) =
                    require_review_bundle_scope(&bundle, paper_id, identity)?;
                let quorum = bundle
                    .get("evaluation_quorum")
                    .filter(|value| !value.is_null())
                    .ok_or(AppError::NotFound)?;
                let draft = quorum.get("draft").ok_or(AppError::Upstream)?;
                if required_uuid(draft, "evaluation_id")? != evaluation_id
                    || required_uuid(draft, "paper_project_id")? != paper_id
                    || required_uuid(draft, "submission_id")? != submission_id
                    || draft.get("status").and_then(Value::as_str) != Some("open")
                {
                    return Err(AppError::Conflict(
                        "review attestation must bind the current open immutable draft".into(),
                    ));
                }
                let review_round = required_u64(draft, "review_round")?;
                let assignment = require_exact_review_assignment(
                    &bundle,
                    identity,
                    submission_id,
                    review_round,
                    &["reviewer_1", "reviewer_2"],
                )?;
                let slot = required_string(assignment, "slot")?;
                let player_id = identity.player_id.to_string();
                if quorum
                    .get("attestations")
                    .and_then(Value::as_array)
                    .is_some_and(|records| {
                        records.iter().any(|record| {
                            record.get("slot").and_then(Value::as_str) == Some(slot.as_str())
                                || record
                                    .get("attestation")
                                    .and_then(|value| value.get("reviewer_player_id"))
                                    .and_then(Value::as_str)
                                    == Some(player_id.as_str())
                        })
                    })
                {
                    return Err(AppError::Conflict(
                        "this reviewer slot already has its immutable attestation".into(),
                    ));
                }
                let evaluation_signing_hash = required_string(draft, "evaluation_signing_hash")?;
                let draft_hash = required_string(draft, "draft_hash")?;
                let signing = PaperReviewAttestationSigningV1 {
                    schema: PAPER_REVIEW_ATTESTATION_V1.into(),
                    attestation_id: input.attestation_id,
                    evaluation_id,
                    evaluation_signing_hash,
                    reviewer_player_id: identity.player_id,
                    verdict: input.verdict.clone(),
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    coi_attestation_hash: input.coi_attestation_hash.clone(),
                    signed_at_unix,
                };
                let bytes =
                    paper_review_attestation_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "attestation_id": input.attestation_id,
                    "draft_hash": draft_hash,
                    "reviewer_player_id": identity.player_id,
                    "verdict": input.verdict,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "coi_attestation_hash": input.coi_attestation_hash,
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::SubmitReproduction => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let evaluation_id = request
                    .child_id
                    .ok_or_else(|| AppError::Invalid("child_id is required".into()))?;
                let input: ReproductionFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid reproduction frame payload".into())
                    })?;
                let bundle = self.get_paper_review_bundle(identity, paper_id).await?;
                let (submission_id, release_candidate_hash, paper_bundle_hash) =
                    require_review_bundle_scope(&bundle, paper_id, identity)?;
                let evaluation = bundle
                    .get("evaluation")
                    .filter(|value| !value.is_null())
                    .ok_or(AppError::NotFound)?;
                if required_uuid(evaluation, "evaluation_id")? != evaluation_id
                    || required_uuid(evaluation, "paper_project_id")? != paper_id
                    || required_uuid(evaluation, "submission_id")? != submission_id
                    || required_string(evaluation, "release_candidate_hash")?
                        != release_candidate_hash
                    || required_string(evaluation, "paper_bundle_hash")? != paper_bundle_hash
                {
                    return Err(AppError::Conflict(
                        "reproduction must bind the current finalized evaluation".into(),
                    ));
                }
                let review_round = required_u64(evaluation, "version")?;
                require_exact_review_assignment(
                    &bundle,
                    identity,
                    submission_id,
                    review_round,
                    &["reproducer"],
                )?;
                let observed_metrics_hash = canonical_value_hash(&input.observed_metrics_micros)?;
                let statistical_evidence_hash = canonical_value_hash(&input.statistical_evidence)?;
                let signing = PaperReproductionSigningV1 {
                    schema: PAPER_REPRODUCTION_V1.into(),
                    reproduction_id: input.reproduction_id,
                    evaluation_id,
                    paper_project_id: paper_id,
                    release_candidate_hash: release_candidate_hash.clone(),
                    paper_bundle_hash: paper_bundle_hash.clone(),
                    tolerance_policy_hash: required_string(evaluation, "tolerance_policy_hash")?,
                    observed_metrics_hash,
                    statistical_evidence_hash,
                    seed_set_hash: input.seed_set_hash.clone(),
                    environment_hash: input.environment_hash.clone(),
                    run_manifest_hash: input.run_manifest_hash.clone(),
                    supersedes_reproduction_id: input.supersedes_reproduction_id,
                    reproducer_player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    coi_attestation_hash: input.coi_attestation_hash.clone(),
                    signed_at_unix,
                };
                let bytes =
                    paper_reproduction_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "reproduction_id": input.reproduction_id,
                    "supersedes_reproduction_id": input.supersedes_reproduction_id,
                    "release_candidate_hash": release_candidate_hash,
                    "paper_bundle_hash": paper_bundle_hash,
                    "observed_metrics_micros": input.observed_metrics_micros,
                    "statistical_evidence": input.statistical_evidence,
                    "seed_set_hash": input.seed_set_hash,
                    "environment_hash": input.environment_hash,
                    "run_manifest_hash": input.run_manifest_hash,
                    "reproducer_player_id": identity.player_id,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "coi_attestation_hash": input.coi_attestation_hash,
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::StartPaperRework => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid("child_id is not allowed".into()));
                }
                let input: PaperReworkFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid Paper rework frame payload".into())
                    })?;
                if input.rework_id.is_nil() || !sealed_digest(&input.reason_hash) {
                    return Err(AppError::Invalid(
                        "Paper rework requires a fresh rework_id and canonical reason_hash".into(),
                    ));
                }

                // The ordinary Author form supplies only a locally generated ID and the
                // locally hashed reason.  Every scientific locator and optimistic version
                // below comes from fresh, authenticated Hepta reads so stale browser state
                // can never choose the rejected submission or evaluation being signed.
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let paper = room
                    .get("paper")
                    .filter(|value| value.is_object())
                    .ok_or(AppError::Upstream)?;
                let expected_paper_version = required_u64(paper, "version")?;
                if expected_paper_version == 0
                    || paper.get("phase").and_then(Value::as_str) != Some("submission_ready")
                    || paper.get("outcome").and_then(Value::as_str) != Some("submission_ready")
                    || paper.get("terminal_at").is_none_or(Value::is_null)
                    || paper.get("active_rework_id") != Some(&Value::Null)
                    || paper.get("active_rework_cycle") != Some(&Value::Null)
                    || paper.get("rework_expires_at") != Some(&Value::Null)
                {
                    return Err(AppError::Conflict(
                        "Paper rework requires the current terminal rejected submission and no active rework lease"
                            .into(),
                    ));
                }
                let submission = room
                    .get("joint_submission")
                    .filter(|value| value.is_object())
                    .ok_or(AppError::NotFound)?;
                let rejected_submission_id = required_uuid(submission, "submission_id")?;
                if required_uuid(submission, "paper_project_id")? != paper_id
                    || submission.get("status").and_then(Value::as_str) != Some("submission_ready")
                {
                    return Err(AppError::Conflict(
                        "Paper rework must bind the exact current submission-ready PaperBundle"
                            .into(),
                    ));
                }

                let review = self.get_paper_review_state(identity, paper_id).await?;
                let evaluation =
                    latest_submission_evaluation(review.value(), paper_id, rejected_submission_id)?;
                if evaluation.get("status").and_then(Value::as_str) != Some("rejected") {
                    return Err(AppError::Conflict(
                        "Paper rework requires the current immutable rejected evaluation".into(),
                    ));
                }
                let rejected_evaluation_id = required_uuid(evaluation, "evaluation_id")?;

                let history = self.get_paper_reworks(identity, paper_id).await?;
                let rework_cycle = next_paper_rework_cycle(&history, paper_id)?;
                let signing = PaperReworkSigningV1 {
                    schema: PAPER_REWORK_V1.into(),
                    rework_id: input.rework_id,
                    paper_project_id: paper_id,
                    rejected_evaluation_id,
                    rejected_submission_id,
                    expected_paper_version,
                    rework_cycle,
                    author_player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    reason_hash: input.reason_hash.clone(),
                    signed_at_unix,
                };
                let bytes = paper_rework_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "rework_id": input.rework_id,
                    "rejected_evaluation_id": rejected_evaluation_id,
                    "rejected_submission_id": rejected_submission_id,
                    "expected_paper_version": expected_paper_version,
                    "rework_cycle": rework_cycle,
                    "author_player_id": identity.player_id,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "reason_hash": input.reason_hash,
                    "signed_at_unix": signed_at_unix,
                });
                (payload, bytes)
            }
            CommandName::SubmitAppeal => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let input: AppealFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| AppError::Invalid("invalid Appeal frame payload".into()))?;
                let guided = request.child_id.is_none();
                let guided_payload = input.evidence_manifest_id.is_some()
                    && input.release_candidate_hash.is_none()
                    && input.evidence_manifest_hash.is_none();
                let advanced_payload = input.evidence_manifest_id.is_none()
                    && input.release_candidate_hash.is_some()
                    && input.evidence_manifest_hash.is_some();
                if (guided && !guided_payload) || (!guided && !advanced_payload) {
                    return Err(AppError::Invalid(
                        "guided Appeal requires one evidence_manifest_id; the Advanced frame requires exact release and manifest hashes"
                            .into(),
                    ));
                }
                let room = self.get_paper_room(identity, paper_id).await?;
                let room = room.value();
                require_room_paper_id(room, paper_id)?;
                let review = self.get_paper_review_state(identity, paper_id).await?;
                let review = review.value();
                if review
                    .get("finality")
                    .and_then(|value| value.get("status"))
                    .and_then(Value::as_str)
                    != Some("pending_finality")
                {
                    return Err(AppError::Conflict(
                        "Appeal signing requires pending scientific finality".into(),
                    ));
                }
                let evaluation = match request.child_id {
                    Some(evaluation_id) => paper_evaluation_by_id(review, paper_id, evaluation_id)?,
                    None => latest_paper_evaluation(review, paper_id)?,
                };
                let evaluation_id = required_uuid(evaluation, "evaluation_id")?;
                let current_release = required_string(evaluation, "release_candidate_hash")?;
                if input
                    .release_candidate_hash
                    .as_deref()
                    .is_some_and(|supplied| supplied != current_release)
                {
                    return Err(AppError::Conflict(
                        "Appeal payload does not match the evaluated release candidate".into(),
                    ));
                }
                let evaluation_id_text = evaluation_id.to_string();
                let prior_appeals = review
                    .get("appeals")
                    .and_then(Value::as_array)
                    .ok_or(AppError::Upstream)?
                    .iter()
                    .filter(|appeal| {
                        appeal.get("evaluation_id").and_then(Value::as_str)
                            == Some(evaluation_id_text.as_str())
                    })
                    .count();
                if prior_appeals != 0 {
                    return Err(AppError::Conflict(
                        "the current evaluation already has an immutable Appeal".into(),
                    ));
                }
                let evidence_manifest_hash = exact_manifest_hash(
                    room,
                    paper_id,
                    input.evidence_manifest_id,
                    input.evidence_manifest_hash.as_deref(),
                )?;
                let signing = PaperAppealSigningV1 {
                    schema: PAPER_APPEAL_V1.into(),
                    appeal_id: input.appeal_id,
                    evaluation_id,
                    paper_project_id: paper_id,
                    release_candidate_hash: current_release.clone(),
                    appellant_player_id: identity.player_id,
                    grounds_hash: input.grounds_hash.clone(),
                    evidence_manifest_hash: evidence_manifest_hash.clone(),
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = paper_appeal_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "appeal_id": input.appeal_id,
                    "release_candidate_hash": current_release,
                    "appellant_player_id": identity.player_id,
                    "grounds_hash": input.grounds_hash,
                    "evidence_manifest_hash": evidence_manifest_hash,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                derived_child_id = Some(evaluation_id);
                (payload, bytes)
            }
            CommandName::ResolveAppeal => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                if request.child_id.is_some() {
                    return Err(AppError::Invalid(
                        "guided Appeal resolution derives child_id from the current review state"
                            .into(),
                    ));
                }
                let input: AppealResolutionFramePayload =
                    serde_json::from_value(request.payload.clone()).map_err(|_| {
                        AppError::Invalid("invalid Appeal resolution frame payload".into())
                    })?;
                if !matches!(input.outcome.as_str(), "denied" | "upheld") {
                    return Err(AppError::Invalid(
                        "Appeal resolution outcome must be denied or upheld".into(),
                    ));
                }
                let bundle = self.get_paper_review_bundle(identity, paper_id).await?;
                let (submission_id, release_candidate_hash, paper_bundle_hash) =
                    require_review_bundle_scope(&bundle, paper_id, identity)?;
                let assigned_evaluation = bundle
                    .get("evaluation")
                    .filter(|value| !value.is_null())
                    .ok_or(AppError::NotFound)?;
                let evaluation_id = required_uuid(assigned_evaluation, "evaluation_id")?;
                let review_round = required_u64(assigned_evaluation, "version")?;
                if required_uuid(assigned_evaluation, "paper_project_id")? != paper_id
                    || required_uuid(assigned_evaluation, "submission_id")? != submission_id
                    || required_string(assigned_evaluation, "release_candidate_hash")?
                        != release_candidate_hash
                    || required_string(assigned_evaluation, "paper_bundle_hash")?
                        != paper_bundle_hash
                {
                    return Err(AppError::Conflict(
                        "Appeal resolver assignment does not bind the current frozen evaluation"
                            .into(),
                    ));
                }
                require_exact_review_assignment(
                    &bundle,
                    identity,
                    submission_id,
                    review_round,
                    &["reproducer"],
                )?;
                let review = self.get_paper_review_state(identity, paper_id).await?;
                let review = review.value();
                if review
                    .get("finality")
                    .and_then(|value| value.get("status"))
                    .and_then(Value::as_str)
                    != Some("pending_finality")
                {
                    return Err(AppError::Conflict(
                        "Appeal resolution requires pending scientific finality".into(),
                    ));
                }
                let evaluation = paper_evaluation_by_id(review, paper_id, evaluation_id)?;
                if required_uuid(evaluation, "submission_id")? != submission_id
                    || required_string(evaluation, "release_candidate_hash")?
                        != release_candidate_hash
                    || required_string(evaluation, "paper_bundle_hash")? != paper_bundle_hash
                    || required_u64(evaluation, "version")? != review_round
                    || evaluation_panel_contains(evaluation, identity.player_id)?
                {
                    return Err(AppError::Forbidden);
                }
                let appeal = exact_open_appeal(review, paper_id, evaluation_id)?;
                if required_uuid(appeal, "evaluation_id")? != evaluation_id
                    || required_string(appeal, "release_candidate_hash")? != release_candidate_hash
                    || required_uuid(appeal, "appellant_player_id")? == identity.player_id
                {
                    return Err(AppError::Forbidden);
                }
                let superseding = exact_superseding_evaluation(review, evaluation)?;
                let superseding_evaluation_id = match input.outcome.as_str() {
                    "denied" => None,
                    "upheld" => {
                        let replacement = superseding.ok_or_else(|| {
                            AppError::Conflict(
                                "upheld Appeal requires one exact eligible superseding evaluation"
                                    .into(),
                            )
                        })?;
                        if evaluation_panel_contains(replacement, identity.player_id)? {
                            return Err(AppError::Forbidden);
                        }
                        Some(required_uuid(replacement, "evaluation_id")?)
                    }
                    _ => unreachable!("validated resolution outcome"),
                };
                let appeal_id = required_uuid(appeal, "appeal_id")?;
                let signing = PaperAppealResolutionSigningV1 {
                    schema: PAPER_APPEAL_RESOLUTION_V1.into(),
                    resolution_id: input.resolution_id,
                    appeal_id,
                    evaluation_id,
                    paper_project_id: paper_id,
                    release_candidate_hash: release_candidate_hash.clone(),
                    outcome: input.outcome.clone(),
                    superseding_evaluation_id,
                    decision_hash: input.decision_hash.clone(),
                    resolver_player_id: identity.player_id,
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes =
                    paper_appeal_resolution_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "resolution_id": input.resolution_id,
                    "outcome": input.outcome,
                    "superseding_evaluation_id": superseding_evaluation_id,
                    "decision_hash": input.decision_hash,
                    "resolver_player_id": identity.player_id,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
                derived_child_id = Some(appeal_id);
                (payload, bytes)
            }
            _ => {
                return Err(AppError::Invalid(
                    "command does not support browser-local human signing".into(),
                ))
            }
        };
        Ok(HumanSigningFrameResponse {
            command: request.command,
            resource_id: derived_resource_id,
            child_id: derived_child_id,
            payload,
            signing_bytes: BASE64.encode(signing_bytes),
            signing_key_id: player.signing_key_id,
            signing_public_key: player.signing_public_key,
            signing_public_key_hash: player.signing_public_key_hash,
        })
    }

    pub async fn get_team(
        &self,
        identity: &AlphaIdentity,
        team_id: Uuid,
    ) -> Result<Value, AppError> {
        self.send(
            identity,
            Route {
                method: Method::GET,
                path: format!("/v2/hepta/teams/{team_id}"),
                query: None,
                operation: "get_research_team_v2",
            },
            Uuid::new_v4(),
            &Value::Null,
        )
        .await
        .and_then(read_json)
    }

    pub async fn get_team_acceptances(
        &self,
        identity: &AlphaIdentity,
        team_id: Uuid,
    ) -> Result<Value, AppError> {
        self.send(
            identity,
            Route {
                method: Method::GET,
                path: format!("/v2/hepta/teams/{team_id}/member-acceptances"),
                query: None,
                operation: "list_research_team_acceptances_v2",
            },
            Uuid::new_v4(),
            &Value::Null,
        )
        .await
        .and_then(read_json)
    }

    pub async fn get_paper(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.send(
            identity,
            Route {
                method: Method::GET,
                path: format!("/v2/hepta/papers/{paper_id}"),
                query: None,
                operation: "get_paper_project_v2",
            },
            Uuid::new_v4(),
            &Value::Null,
        )
        .await
        .and_then(read_json)
    }

    pub async fn get_submission(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.send(
            identity,
            Route {
                method: Method::GET,
                path: format!("/v2/hepta/papers/{paper_id}/submission"),
                query: None,
                operation: "get_joint_paper_submission_v2",
            },
            Uuid::new_v4(),
            &Value::Null,
        )
        .await
        .and_then(read_json)
    }

    pub async fn list_public_challenges(
        &self,
        identity: &AlphaIdentity,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/challenges".into(),
            None,
            "list_public_challenges_v2",
        )
        .await
    }

    pub async fn list_matchmaking_tickets(
        &self,
        identity: &AlphaIdentity,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/matchmaking/tickets".into(),
            None,
            "list_matchmaking_tickets_v3",
        )
        .await
    }

    pub async fn list_team_proposals(&self, identity: &AlphaIdentity) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/team-proposals".into(),
            None,
            "list_team_proposals_v3",
        )
        .await
    }

    pub async fn get_team_proposal(
        &self,
        identity: &AlphaIdentity,
        proposal_id: Uuid,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/team-proposals/{proposal_id}"),
            None,
            "get_team_proposal_v3",
        )
        .await
    }

    pub async fn get_player_raid_state(&self, identity: &AlphaIdentity) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/raid-state".into(),
            None,
            "get_player_raid_state_v1",
        )
        .await
    }

    pub(crate) async fn get_paper_room(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<AuthenticatedPaperRoom, AppError> {
        let route = Route {
            method: Method::GET,
            path: format!("/v2/hepta/papers/{paper_id}/room"),
            query: None,
            operation: "get_paper_room_v3",
        };
        let body = self
            .send(identity, route.clone(), Uuid::new_v4(), &Value::Null)
            .await
            .and_then(read_json)?;
        AuthenticatedPaperRoom::seal(&route, paper_id, body)
    }

    pub async fn list_paper_room_events(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
        after_cursor: u64,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/papers/{paper_id}/events"),
            Some(format!("after_cursor={after_cursor}")),
            "list_paper_room_events_v3",
        )
        .await
    }

    pub(crate) async fn get_paper_review_state(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<AuthenticatedPaperReviewState, AppError> {
        let route = Route {
            method: Method::GET,
            path: format!("/v2/hepta/papers/{paper_id}/review-state"),
            query: None,
            operation: "get_paper_review_state_v1",
        };
        let body = self
            .send(identity, route.clone(), Uuid::new_v4(), &Value::Null)
            .await
            .and_then(read_json)?;
        AuthenticatedPaperReviewState::seal(&route, paper_id, body)
    }

    pub(crate) async fn get_paper_reworks(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/papers/{paper_id}/reworks"),
            None,
            "get_paper_reworks_v1",
        )
        .await
    }

    pub async fn list_review_queue(&self, identity: &AlphaIdentity) -> Result<Value, AppError> {
        self.get_json(
            identity,
            "/v2/hepta/review-queue".into(),
            None,
            "get_review_queue_v1",
        )
        .await
    }

    pub async fn get_paper_review_bundle(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/papers/{paper_id}/review-bundle"),
            None,
            "get_paper_review_bundle_v1",
        )
        .await
    }

    async fn get_json(
        &self,
        identity: &AlphaIdentity,
        path: String,
        query: Option<String>,
        operation: &'static str,
    ) -> Result<Value, AppError> {
        self.send(
            identity,
            Route {
                method: Method::GET,
                path,
                query,
                operation,
            },
            Uuid::new_v4(),
            &Value::Null,
        )
        .await
        .and_then(read_json)
    }

    pub async fn forward_command(
        &self,
        identity: &AlphaIdentity,
        command: &BrowserCommand,
    ) -> Result<UpstreamResponse, AppError> {
        let route = command.route()?;
        let payload = command.canonical_payload()?;
        validate_identity_command_payload(identity, command.command, &payload)?;
        self.send(identity, route, command.idempotency_key, &payload)
            .await
    }

    async fn send(
        &self,
        identity: &AlphaIdentity,
        route: Route,
        idempotency_key: Uuid,
        body: &Value,
    ) -> Result<UpstreamResponse, AppError> {
        if !route.path.starts_with("/v2/hepta/") {
            return Err(AppError::Internal);
        }
        if route.query.as_deref().is_some_and(|query| {
            query.is_empty()
                || query.bytes().any(|byte| {
                    !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'=' | b'&' | b'-'))
                })
        }) {
            return Err(AppError::Internal);
        }
        let canonical_path = route.canonical_path();
        let body_bytes = if route.method == Method::GET {
            Vec::new()
        } else {
            canonical_json_bytes(body).map_err(AppError::Invalid)?
        };
        if body_bytes.len() > MAX_JSON_BYTES {
            return Err(AppError::Invalid("command body is too large".into()));
        }
        let request_hash = business_request_hash(&route, &body_bytes);
        if route.method != Method::GET {
            if let Some(replay) = self
                .begin_idempotent(identity, idempotency_key, &request_hash)
                .await?
            {
                return Ok(replay);
            }
        }
        let now = Utc::now().timestamp();
        let assertion_id = Uuid::new_v4();
        let inserted = sqlx::query(
            "INSERT INTO paper_raid_bff_assertions(assertion_id, subject_id, expires_at) \
             VALUES ($1, $2, to_timestamp($3)) ON CONFLICT DO NOTHING",
        )
        .bind(assertion_id)
        .bind(&identity.subject_id)
        .bind(now + self.assertions.ttl.as_secs() as i64)
        .execute(&self.pool)
        .await?;
        if inserted.rows_affected() != 1 {
            return Err(AppError::Internal);
        }

        let claim = ConsumerUserAssertionClaimV2 {
            schema: CONSUMER_USER_ASSERTION_V2.into(),
            assertion_id,
            issuer: self.assertions.issuer.clone(),
            audience: self.assertions.audience.clone(),
            subject_id: identity.subject_id.clone(),
            nakama_user_id: identity.nakama_user_id,
            player_id: identity.player_id,
            operation: route.operation.into(),
            http_method: route.method.as_str().into(),
            canonical_path: canonical_path.clone(),
            idempotency_key: idempotency_key.to_string(),
            body_hash: sha256_digest(&body_bytes),
            issued_at_unix: now,
            expires_at_unix: now + self.assertions.ttl.as_secs() as i64,
            nonce: idempotency_key.to_string(),
        };
        let signed = sign_consumer_user_assertion(
            claim,
            &self.assertions.key_id,
            &self.assertions.signing_key,
        )
        .map_err(AppError::Invalid)?;
        let assertion = BASE64.encode(canonical_json_bytes(&signed).map_err(AppError::Invalid)?);
        let mut url = join_exact(&self.base, &route.path).map_err(AppError::Invalid)?;
        url.set_query(route.query.as_deref());
        let mut request = self
            .client
            .request(route.method.clone(), url)
            .header(ASSERTION_HEADER, assertion)
            .header(header::ACCEPT, "application/json");
        if route.method != Method::GET {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(body_bytes);
        }
        let response = request.send().await.map_err(|_| {
            self.metrics.observe_hepta_error(HeptaErrorKind::Transport);
            AppError::Unavailable("hepta")
        })?;
        if response.status().is_redirection() || !strict_json(response.headers()) {
            self.metrics.observe_hepta_error(HeptaErrorKind::Protocol);
            return Err(AppError::Upstream);
        }
        let status = response.status();
        let bytes = limited_body(response, MAX_JSON_BYTES)
            .await
            .inspect_err(|_| self.metrics.observe_hepta_error(HeptaErrorKind::Protocol))?;
        let _: Value = serde_json::from_slice(&bytes).map_err(|_| {
            self.metrics.observe_hepta_error(HeptaErrorKind::Protocol);
            AppError::Upstream
        })?;
        if status.is_server_error() {
            self.metrics.observe_hepta_error(HeptaErrorKind::Server);
            return Err(AppError::Unavailable("hepta"));
        }
        if route.method != Method::GET && status.is_success() {
            self.complete_idempotent(
                identity,
                idempotency_key,
                &request_hash,
                status.as_u16(),
                &bytes,
            )
            .await?;
        }
        Ok(UpstreamResponse {
            status: status.as_u16(),
            body: bytes,
            replayed: false,
        })
    }

    async fn begin_idempotent(
        &self,
        identity: &AlphaIdentity,
        idempotency_key: Uuid,
        request_hash: &[u8; 32],
    ) -> Result<Option<UpstreamResponse>, AppError> {
        sqlx::query(
            "INSERT INTO paper_raid_bff_idempotency \
             (subject_id, idempotency_key, request_hash, state) VALUES ($1, $2, $3, 'pending') \
             ON CONFLICT (subject_id, idempotency_key) DO NOTHING",
        )
        .bind(&identity.subject_id)
        .bind(idempotency_key)
        .bind(request_hash.as_slice())
        .execute(&self.pool)
        .await?;
        let row = sqlx::query(
            "SELECT request_hash, state, response_status, response_body \
             FROM paper_raid_bff_idempotency WHERE subject_id = $1 AND idempotency_key = $2",
        )
        .bind(&identity.subject_id)
        .bind(idempotency_key)
        .fetch_one(&self.pool)
        .await?;
        let stored_hash: Vec<u8> = row
            .try_get("request_hash")
            .map_err(|_| AppError::Internal)?;
        if stored_hash.as_slice() != request_hash {
            return Err(AppError::Conflict(
                "idempotency_key was already used for a different request".into(),
            ));
        }
        let state: String = row.try_get("state").map_err(|_| AppError::Internal)?;
        if state == "completed" {
            let status: i32 = row
                .try_get("response_status")
                .map_err(|_| AppError::Internal)?;
            let body: Vec<u8> = row
                .try_get("response_body")
                .map_err(|_| AppError::Internal)?;
            return Ok(Some(UpstreamResponse {
                status: u16::try_from(status).map_err(|_| AppError::Internal)?,
                body,
                replayed: true,
            }));
        }
        Ok(None)
    }

    async fn complete_idempotent(
        &self,
        identity: &AlphaIdentity,
        idempotency_key: Uuid,
        request_hash: &[u8; 32],
        status: u16,
        body: &[u8],
    ) -> Result<(), AppError> {
        let updated = sqlx::query(
            "UPDATE paper_raid_bff_idempotency \
             SET state = 'completed', response_status = $1, response_body = $2, completed_at = now() \
             WHERE subject_id = $3 AND idempotency_key = $4 AND request_hash = $5 AND state = 'pending'",
        )
        .bind(i32::from(status))
        .bind(body)
        .bind(&identity.subject_id)
        .bind(idempotency_key)
        .bind(request_hash.as_slice())
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 1 {
            return Ok(());
        }
        let replay = self
            .begin_idempotent(identity, idempotency_key, request_hash)
            .await?
            .ok_or(AppError::Internal)?;
        if replay.status != status || replay.body != body {
            return Err(AppError::Conflict(
                "upstream returned inconsistent idempotent responses".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn matchmaking_party_payload_is_safe(payload: &Value) -> bool {
    let Some(object) = payload.as_object() else {
        return false;
    };
    const ALLOWED_KEYS: [&str; 7] = [
        "ticket_id",
        "challenge_id",
        "requested_team_size",
        "roles",
        "availability_hash",
        "party_code_hash",
        "idempotency_key",
    ];
    if object
        .keys()
        .any(|key| !ALLOWED_KEYS.contains(&key.as_str()))
    {
        return false;
    }
    match object.get("party_code_hash") {
        None => true,
        Some(Value::String(value)) => value.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }),
        Some(_) => false,
    }
}

fn validate_identity_command_payload(
    identity: &AlphaIdentity,
    command: CommandName,
    payload: &Value,
) -> Result<(), AppError> {
    if matches!(
        command,
        CommandName::ClaimReviewAssignment
            | CommandName::CreatePaperEvaluation
            | CommandName::CreatePaperEvaluationDraft
            | CommandName::SubmitEvaluationDraftAttestation
            | CommandName::FinalizePaperEvaluationDraft
            | CommandName::SubmitReproduction
            | CommandName::ResolveAppeal
    ) && identity.has_scope(AlphaIdentityScope::Author)
    {
        return Err(AppError::Forbidden);
    }
    match command {
        CommandName::QueueMatchmaking => {
            if !matchmaking_party_payload_is_safe(payload) {
                return Err(AppError::Invalid(
                    "matchmaking accepts only an optional canonical party_code_hash; raw party codes and unknown fields are forbidden"
                        .into(),
                ));
            }
            let roles = payload
                .get("roles")
                .and_then(Value::as_array)
                .filter(|roles| !roles.is_empty() && roles.len() <= 3)
                .ok_or_else(|| {
                    AppError::Invalid("roles must contain 1 to 3 authorized roles".into())
                })?;
            let mut accepted = Vec::with_capacity(roles.len());
            for value in roles {
                let role = match value.as_str() {
                    Some("captain") => AlphaAuthorRole::Captain,
                    Some("evidence") => AlphaAuthorRole::Evidence,
                    Some("experiment") => AlphaAuthorRole::Experiment,
                    _ => return Err(AppError::Forbidden),
                };
                if accepted.contains(&role) || !identity.supports_author_role(role) {
                    return Err(AppError::Forbidden);
                }
                accepted.push(role);
            }
        }
        CommandName::ClaimReviewAssignment => {
            let player_id = payload
                .get("player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or_else(|| AppError::Invalid("claim player_id must be a UUID".into()))?;
            if player_id != identity.player_id {
                return Err(AppError::Forbidden);
            }
            let required_scope = match payload.get("slot").and_then(Value::as_str) {
                Some("evaluator") => AlphaIdentityScope::Evaluator,
                Some("reviewer_1" | "reviewer_2") => AlphaIdentityScope::Reviewer,
                Some("reproducer") => AlphaIdentityScope::Reproducer,
                _ => return Err(AppError::Forbidden),
            };
            if !identity.has_scope(required_scope) {
                return Err(AppError::Forbidden);
            }
        }
        CommandName::CreatePaperEvaluationDraft => {
            require_payload_player(identity, payload, "evaluator_player_id")?;
        }
        CommandName::SubmitEvaluationDraftAttestation => {
            require_payload_player(identity, payload, "reviewer_player_id")?;
        }
        CommandName::SubmitReproduction => {
            require_payload_player(identity, payload, "reproducer_player_id")?;
        }
        CommandName::SubmitAppeal => {
            require_payload_player(identity, payload, "appellant_player_id")?;
        }
        CommandName::StartPaperRework => {
            require_payload_player(identity, payload, "author_player_id")?;
        }
        CommandName::ResolveAppeal => {
            require_payload_player(identity, payload, "resolver_player_id")?;
        }
        _ => {}
    }
    Ok(())
}

fn require_payload_player(
    identity: &AlphaIdentity,
    payload: &Value,
    field: &str,
) -> Result<(), AppError> {
    let player_id = payload
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| AppError::Invalid(format!("{field} must be a UUID")))?;
    if player_id != identity.player_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn validate_frame_https_uri(value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 480 || value.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(AppError::Invalid(
            "source URI must be a bounded canonical HTTPS URL".into(),
        ));
    }
    let parsed = Url::parse(value)
        .map_err(|_| AppError::Invalid("source URI must be a canonical HTTPS URL".into()))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::Invalid(
            "source URI must be credential-free HTTPS without a fragment".into(),
        ));
    }
    Ok(())
}

fn require_room_paper_id(room: &Value, paper_id: Uuid) -> Result<(), AppError> {
    let expected = paper_id.to_string();
    if room
        .get("paper")
        .and_then(|paper| paper.get("paper_project_id"))
        .and_then(Value::as_str)
        == Some(expected.as_str())
    {
        Ok(())
    } else {
        Err(AppError::Upstream)
    }
}

fn required_string(value: &Value, field: &str) -> Result<String, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(AppError::Upstream)
}

fn required_u64(value: &Value, field: &str) -> Result<u64, AppError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(AppError::Upstream)
}

fn required_uuid(value: &Value, field: &str) -> Result<Uuid, AppError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(AppError::Upstream)
}

fn find_room_record<'a>(
    room: &'a Value,
    collection: &str,
    id_field: &str,
    id: Uuid,
) -> Result<&'a Value, AppError> {
    let expected = id.to_string();
    room.get(collection)
        .and_then(Value::as_array)
        .and_then(|records| {
            records.iter().find(|record| {
                record.get(id_field).and_then(Value::as_str) == Some(expected.as_str())
            })
        })
        .ok_or(AppError::NotFound)
}

fn latest_paper_evaluation(review: &Value, paper_id: Uuid) -> Result<&Value, AppError> {
    let evaluations = review
        .get("evaluations")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let mut scoped = Vec::new();
    for evaluation in evaluations {
        if required_uuid(evaluation, "paper_project_id")? != paper_id {
            return Err(AppError::Upstream);
        }
        let _ = required_uuid(evaluation, "evaluation_id")?;
        let _ = required_u64(evaluation, "version")?;
        let _ = required_string(evaluation, "release_candidate_hash")?;
        scoped.push(evaluation);
    }
    let latest_version = scoped
        .iter()
        .map(|evaluation| required_u64(evaluation, "version"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or(AppError::NotFound)?;
    let latest = scoped
        .into_iter()
        .filter(|evaluation| {
            evaluation.get("version").and_then(Value::as_u64) == Some(latest_version)
        })
        .collect::<Vec<_>>();
    if latest.len() != 1 {
        return Err(AppError::Conflict(
            "the current Paper evaluation is ambiguous".into(),
        ));
    }
    Ok(latest[0])
}

fn latest_submission_evaluation(
    review: &Value,
    paper_id: Uuid,
    submission_id: Uuid,
) -> Result<&Value, AppError> {
    let evaluations = review
        .get("evaluations")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let mut scoped = Vec::new();
    for evaluation in evaluations {
        if required_uuid(evaluation, "paper_project_id")? != paper_id {
            return Err(AppError::Upstream);
        }
        let evaluation_id = required_uuid(evaluation, "evaluation_id")?;
        let version = required_u64(evaluation, "version")?;
        if version == 0 {
            return Err(AppError::Upstream);
        }
        if required_uuid(evaluation, "submission_id")? == submission_id {
            scoped.push((version, evaluation_id, evaluation));
        }
    }
    let latest_version = scoped
        .iter()
        .map(|(version, _, _)| *version)
        .max()
        .ok_or(AppError::NotFound)?;
    let current = scoped
        .iter()
        .filter(|(version, _, _)| *version == latest_version)
        .collect::<Vec<_>>();
    if current.len() != 1 {
        return Err(AppError::Conflict(
            "the current rejected evaluation is ambiguous".into(),
        ));
    }
    let (_, evaluation_id, latest) = *current[0];
    let evaluation_id_text = evaluation_id.to_string();
    if scoped.iter().any(|(_, _, candidate)| {
        candidate
            .get("supersedes_evaluation_id")
            .and_then(Value::as_str)
            == Some(evaluation_id_text.as_str())
    }) {
        return Err(AppError::Conflict(
            "the rejected evaluation has already been superseded".into(),
        ));
    }
    Ok(latest)
}

fn next_paper_rework_cycle(history: &Value, paper_id: Uuid) -> Result<u64, AppError> {
    let records = history.as_array().ok_or(AppError::Upstream)?;
    let mut ids = HashSet::with_capacity(records.len());
    let mut cycles = HashSet::with_capacity(records.len());
    let mut max_cycle = 1_u64;
    for state in records {
        let state = state.as_object().ok_or(AppError::Upstream)?;
        if state.len() != 2 {
            return Err(AppError::Upstream);
        }
        let rework = state
            .get("rework")
            .filter(|value| value.is_object())
            .ok_or(AppError::Upstream)?;
        let rework_id = required_uuid(rework, "rework_id")?;
        let cycle = required_u64(rework, "rework_cycle")?;
        let rejected_commitment =
            required_string(rework, "rejected_rework_content_commitment_sha256")?;
        if rework.get("schema").and_then(Value::as_str) != Some("hepta.paper_raid.rework_record.v1")
            || required_uuid(rework, "paper_project_id")? != paper_id
            || cycle < 2
            || !sealed_nonzero_digest(&rejected_commitment)
            || !ids.insert(rework_id)
            || !cycles.insert(cycle)
        {
            return Err(AppError::Upstream);
        }
        let resubmission = state.get("resubmission").ok_or(AppError::Upstream)?;
        if resubmission.is_null() {
            return Err(AppError::Conflict(
                "Paper already has an active replacement workflow".into(),
            ));
        }
        if resubmission.get("schema").and_then(Value::as_str)
            != Some("hepta.paper_raid.rework_resubmission.v1")
            || required_uuid(resubmission, "rework_id")? != rework_id
            || required_uuid(resubmission, "paper_project_id")? != paper_id
            || required_u64(resubmission, "replacement_review_round")? != 1
        {
            return Err(AppError::Upstream);
        }
        let replacement_commitment =
            required_string(resubmission, "replacement_rework_content_commitment_sha256")?;
        if !sealed_nonzero_digest(&replacement_commitment)
            || replacement_commitment == rejected_commitment
        {
            return Err(AppError::Upstream);
        }
        max_cycle = max_cycle.max(cycle);
    }
    if (2..=max_cycle).any(|cycle| !cycles.contains(&cycle)) {
        return Err(AppError::Upstream);
    }
    max_cycle
        .checked_add(1)
        .ok_or_else(|| AppError::Conflict("Paper rework cycle overflow".into()))
}

fn paper_evaluation_by_id(
    review: &Value,
    paper_id: Uuid,
    evaluation_id: Uuid,
) -> Result<&Value, AppError> {
    let wanted = evaluation_id.to_string();
    let matches = review
        .get("evaluations")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?
        .iter()
        .filter(|evaluation| {
            evaluation.get("evaluation_id").and_then(Value::as_str) == Some(wanted.as_str())
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(if matches.is_empty() {
            AppError::NotFound
        } else {
            AppError::Upstream
        });
    }
    if required_uuid(matches[0], "paper_project_id")? != paper_id {
        return Err(AppError::Conflict(
            "evaluation does not belong to the requested Paper".into(),
        ));
    }
    Ok(matches[0])
}

fn exact_open_appeal(
    review: &Value,
    paper_id: Uuid,
    evaluation_id: Uuid,
) -> Result<&Value, AppError> {
    let resolutions = review
        .get("resolutions")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let appeal_id_is_resolved = |appeal_id: &str| {
        resolutions.iter().any(|resolution| {
            resolution.get("appeal_id").and_then(Value::as_str) == Some(appeal_id)
        })
    };
    let evaluation = evaluation_id.to_string();
    let mut open = Vec::new();
    for appeal in review
        .get("appeals")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?
    {
        if required_uuid(appeal, "paper_project_id")? != paper_id {
            return Err(AppError::Upstream);
        }
        if appeal.get("evaluation_id").and_then(Value::as_str) != Some(evaluation.as_str()) {
            continue;
        }
        let appeal_id = required_string(appeal, "appeal_id")?;
        if !appeal_id_is_resolved(&appeal_id) {
            open.push(appeal);
        }
    }
    if open.len() != 1 {
        return Err(if open.is_empty() {
            AppError::NotFound
        } else {
            AppError::Conflict("multiple unresolved Appeals are visible".into())
        });
    }
    Ok(open[0])
}

fn evaluation_panel_ids(evaluation: &Value) -> Result<HashSet<Uuid>, AppError> {
    let mut panel = HashSet::from([required_uuid(evaluation, "evaluator_player_id")?]);
    let reviewers = evaluation
        .get("reviewer_attestations")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    if reviewers.len() != 2 {
        return Err(AppError::Upstream);
    }
    for attestation in reviewers {
        panel.insert(required_uuid(attestation, "reviewer_player_id")?);
    }
    if panel.len() != 3 {
        return Err(AppError::Upstream);
    }
    Ok(panel)
}

fn evaluation_panel_contains(evaluation: &Value, player_id: Uuid) -> Result<bool, AppError> {
    Ok(evaluation_panel_ids(evaluation)?.contains(&player_id))
}

fn exact_superseding_evaluation<'a>(
    review: &'a Value,
    appealed: &Value,
) -> Result<Option<&'a Value>, AppError> {
    let appealed_id = required_uuid(appealed, "evaluation_id")?;
    let appealed_id_text = appealed_id.to_string();
    let paper_id = required_uuid(appealed, "paper_project_id")?;
    let submission_id = required_uuid(appealed, "submission_id")?;
    let release_candidate_hash = required_string(appealed, "release_candidate_hash")?;
    let paper_bundle_hash = required_string(appealed, "paper_bundle_hash")?;
    let expected_version = required_u64(appealed, "version")?
        .checked_add(1)
        .ok_or(AppError::Upstream)?;
    let candidates = review
        .get("evaluations")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?
        .iter()
        .filter(|evaluation| {
            evaluation
                .get("supersedes_evaluation_id")
                .and_then(Value::as_str)
                == Some(appealed_id_text.as_str())
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(None);
    }
    if candidates.len() != 1 {
        return Err(AppError::Conflict(
            "the superseding evaluation lineage is ambiguous".into(),
        ));
    }
    let candidate = candidates[0];
    if required_uuid(candidate, "paper_project_id")? != paper_id
        || required_uuid(candidate, "submission_id")? != submission_id
        || required_string(candidate, "release_candidate_hash")? != release_candidate_hash
        || required_string(candidate, "paper_bundle_hash")? != paper_bundle_hash
        || required_u64(candidate, "version")? != expected_version
    {
        return Err(AppError::Conflict(
            "the superseding evaluation does not bind the exact appealed release".into(),
        ));
    }
    if !evaluation_panel_ids(appealed)?.is_disjoint(&evaluation_panel_ids(candidate)?) {
        return Err(AppError::Conflict(
            "the superseding evaluation panel is not independent".into(),
        ));
    }
    Ok(Some(candidate))
}

fn exact_manifest_hash(
    room: &Value,
    paper_id: Uuid,
    manifest_id: Option<Uuid>,
    manifest_hash: Option<&str>,
) -> Result<String, AppError> {
    let manifests = room
        .get("artifact_manifests")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let matches = manifests
        .iter()
        .filter(|manifest| {
            let id_matches = manifest_id.is_none_or(|wanted| {
                manifest
                    .get("manifest_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                    == Some(wanted)
            });
            let hash_matches = manifest_hash.is_none_or(|wanted| {
                manifest.get("manifest_hash").and_then(Value::as_str) == Some(wanted)
            });
            id_matches && hash_matches
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(if matches.is_empty() {
            AppError::NotFound
        } else {
            AppError::Conflict("the selected evidence manifest is ambiguous".into())
        });
    }
    if required_uuid(matches[0], "paper_project_id")? != paper_id {
        return Err(AppError::Conflict(
            "the selected evidence manifest belongs to another Paper".into(),
        ));
    }
    required_string(matches[0], "manifest_hash")
}

fn canonical_value_hash<T: Serialize>(value: &T) -> Result<String, AppError> {
    Ok(sha256_digest(
        &canonical_json_bytes(value).map_err(AppError::Invalid)?,
    ))
}

fn require_review_bundle_scope(
    bundle: &Value,
    paper_id: Uuid,
    identity: &AlphaIdentity,
) -> Result<(Uuid, String, String), AppError> {
    if required_uuid(bundle, "paper_project_id")? != paper_id
        || bundle.get("status").and_then(Value::as_str) != Some("submission_ready")
    {
        return Err(AppError::Conflict(
            "review action must bind the current submission-ready PaperBundle".into(),
        ));
    }
    let player_id = identity.player_id.to_string();
    let is_author = bundle
        .get("paper_bundle")
        .and_then(|value| value.get("release_candidate"))
        .and_then(|value| value.get("authors"))
        .and_then(Value::as_array)
        .is_some_and(|authors| {
            authors.iter().any(|author| {
                author.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
            })
        });
    if is_author || identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    Ok((
        required_uuid(bundle, "submission_id")?,
        required_string(bundle, "release_candidate_hash")?,
        required_string(bundle, "paper_bundle_hash")?,
    ))
}

fn require_exact_review_assignment<'a>(
    bundle: &'a Value,
    identity: &AlphaIdentity,
    submission_id: Uuid,
    review_round: u64,
    allowed_slots: &[&str],
) -> Result<&'a Value, AppError> {
    let player_id = identity.player_id.to_string();
    let submission_id = submission_id.to_string();
    let matches = bundle
        .get("my_assignments")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?
        .iter()
        .filter(|assignment| {
            assignment.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
                && assignment.get("submission_id").and_then(Value::as_str)
                    == Some(submission_id.as_str())
                && assignment.get("review_round").and_then(Value::as_u64) == Some(review_round)
                && assignment
                    .get("slot")
                    .and_then(Value::as_str)
                    .is_some_and(|slot| allowed_slots.contains(&slot))
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::Forbidden);
    }
    Ok(matches[0])
}

fn build_human_registration_challenge(
    identity: &AlphaIdentity,
    signing_public_key: &str,
    issued_at_unix: i64,
    now_unix: i64,
) -> Result<HumanRegistrationChallenge, AppError> {
    if issued_at_unix < 0
        || issued_at_unix % HUMAN_REGISTRATION_BUCKET_SECONDS != 0
        || now_unix < issued_at_unix
        || now_unix >= issued_at_unix + HUMAN_REGISTRATION_TTL_SECONDS
    {
        return Err(AppError::Forbidden);
    }
    let public_key = BASE64.decode(signing_public_key).map_err(|_| {
        AppError::Invalid("human signing public key must be canonical base64".into())
    })?;
    if BASE64.encode(&public_key) != signing_public_key || public_key.len() != 32 {
        return Err(AppError::Invalid(
            "human signing public key must be canonical padded base64 for 32 bytes".into(),
        ));
    }
    let public_key_bytes: [u8; 32] = public_key
        .as_slice()
        .try_into()
        .map_err(|_| AppError::Internal)?;
    VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|_| AppError::Invalid("human signing public key is not Ed25519".into()))?;
    let signing_public_key_hash = sha256_digest(&public_key_bytes);
    let signing_key_id = format!(
        "human-ed25519:{}",
        signing_public_key_hash
            .strip_prefix("sha256:")
            .ok_or(AppError::Internal)?
    );
    let idempotency_key =
        human_registration_idempotency(identity, &public_key_bytes, issued_at_unix)?;
    let mut challenge = HumanRegistrationChallenge {
        schema: HUMAN_KEY_REGISTRATION_V2.into(),
        player_id: identity.player_id,
        subject_id: identity.subject_id.clone(),
        nakama_user_id: identity.nakama_user_id,
        display_name: identity.display_name.clone(),
        signing_key_id,
        signing_public_key: signing_public_key.to_string(),
        signing_public_key_hash,
        idempotency_key,
        issued_at_unix,
        expires_at_unix: issued_at_unix + HUMAN_REGISTRATION_TTL_SECONDS,
        signing_bytes: String::new(),
    };
    challenge.signing_bytes = BASE64.encode(
        human_key_registration_signing_bytes(&human_registration_claim(&challenge))
            .map_err(AppError::Invalid)?,
    );
    Ok(challenge)
}

fn registration_challenge_from_proof(
    identity: &AlphaIdentity,
    proof: &HumanRegistrationProof,
    validation_time_unix: i64,
) -> Result<HumanRegistrationChallenge, AppError> {
    let challenge = build_human_registration_challenge(
        identity,
        &proof.signing_public_key,
        proof.issued_at_unix,
        validation_time_unix,
    )?;
    if challenge.expires_at_unix != proof.expires_at_unix
        || challenge.idempotency_key != proof.idempotency_key
    {
        return Err(AppError::Forbidden);
    }
    verify_human_key_registration_pop(
        &human_registration_claim(&challenge),
        &proof.key_proof_signature,
    )
    .map_err(|_| AppError::Forbidden)?;
    Ok(challenge)
}

fn validate_recovered_human_player(
    identity: &AlphaIdentity,
    challenge: &HumanRegistrationChallenge,
    value: &Value,
) -> Result<(), AppError> {
    let player: PlayerSnapshot =
        serde_json::from_value(value.clone()).map_err(|_| AppError::Upstream)?;
    if player.player_id != identity.player_id
        || player.subject_id != identity.subject_id
        || player.signing_key_id != challenge.signing_key_id
        || player.signing_public_key != challenge.signing_public_key
        || player.signing_public_key_hash != challenge.signing_public_key_hash
    {
        return Err(AppError::Conflict(
            "the registered human signing key differs from this recovery proof".into(),
        ));
    }
    Ok(())
}

fn human_registration_claim(challenge: &HumanRegistrationChallenge) -> HumanKeyRegistrationClaimV2 {
    HumanKeyRegistrationClaimV2 {
        schema: challenge.schema.clone(),
        player_id: challenge.player_id,
        subject_id: challenge.subject_id.clone(),
        nakama_user_id: challenge.nakama_user_id,
        signing_key_id: challenge.signing_key_id.clone(),
        signing_public_key: challenge.signing_public_key.clone(),
        signing_public_key_hash: challenge.signing_public_key_hash.clone(),
        nonce: challenge.idempotency_key.to_string(),
        issued_at_unix: challenge.issued_at_unix,
        expires_at_unix: challenge.expires_at_unix,
    }
}

fn human_registration_idempotency(
    identity: &AlphaIdentity,
    public_key: &[u8; 32],
    issued_at_unix: i64,
) -> Result<Uuid, AppError> {
    fn field(hasher: &mut Sha256, value: &[u8]) -> Result<(), AppError> {
        let length = u32::try_from(value.len()).map_err(|_| AppError::Internal)?;
        hasher.update(length.to_be_bytes());
        hasher.update(value);
        Ok(())
    }

    let mut hasher = Sha256::new();
    hasher.update(b"paper_raid_bff_human_registration_idempotency_v1\0");
    field(&mut hasher, identity.subject_id.as_bytes())?;
    field(&mut hasher, identity.player_id.as_bytes())?;
    field(&mut hasher, identity.nakama_user_id.as_bytes())?;
    field(&mut hasher, public_key)?;
    hasher.update(issued_at_unix.to_be_bytes());
    let digest = hasher.finalize();
    let mut uuid_bytes: [u8; 16] = digest[..16].try_into().map_err(|_| AppError::Internal)?;
    uuid_bytes[6] = (uuid_bytes[6] & 0x0f) | 0x50;
    uuid_bytes[8] = (uuid_bytes[8] & 0x3f) | 0x80;
    Ok(Uuid::from_bytes(uuid_bytes))
}

fn business_request_hash(route: &Route, body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(route.method.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(route.canonical_path().as_bytes());
    hasher.update([0]);
    hasher.update(body);
    hasher.finalize().into()
}

fn read_json(response: UpstreamResponse) -> Result<Value, AppError> {
    match response.status {
        200..=299 => response.json(),
        401 | 403 => Err(AppError::Forbidden),
        404 => Err(AppError::NotFound),
        _ => Err(AppError::Upstream),
    }
}

fn validate_logical_id(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 128
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(AppError::Invalid("session_id is invalid".into()));
    }
    Ok(())
}

fn join_exact(base: &Url, path: &str) -> Result<Url, String> {
    if !path.starts_with('/')
        || path.contains('?')
        || path.contains('#')
        || path.contains("..")
        || path.contains("//")
    {
        return Err("canonical path is invalid".to_string());
    }
    let mut url = base.clone();
    url.set_path(&format!("{}{}", base.path().trim_end_matches('/'), path));
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn strict_json(headers: &header::HeaderMap) -> bool {
    let Some(value) = headers.get(header::CONTENT_TYPE) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    matches!(
        value,
        "application/json" | "application/json; charset=utf-8"
    )
}

async fn limited_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, AppError> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(AppError::Upstream);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| AppError::Upstream)? {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(AppError::Upstream);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Bytes,
        extract::{OriginalUri, Path, Query, State},
        http::{HeaderMap, StatusCode as AxumStatus},
        response::{IntoResponse, Response},
        routing::{get, post},
        Json, Router,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use hepta_paper_raid_contracts::{
        verify_consumer_user_assertion_signature, SignedConsumerUserAssertionV2,
    };
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    fn sealed_review_test_envelope(
        assignments: Vec<Value>,
        evaluation_drafts: Vec<Value>,
    ) -> Value {
        serde_json::json!({
            "finality": {
                "schema": "hepta.paper_raid.consumer_finality.v2",
                "status": "pending_finality",
                "effective_evaluation_id": null,
                "effective_reproduction_id": null,
                "effective_appeal_resolution_id": null,
                "ranking_eligible": false,
                "reward_eligible": false,
                "score_eligible": false,
                "economic_eligible": false,
                "verified_at": null
            },
            "assignments": assignments,
            "evaluation_drafts": evaluation_drafts,
            "contribution_ledgers": [],
            "evaluations": [],
            "reproductions": [],
            "appeals": [],
            "resolutions": [],
            "raid_scores": []
        })
    }

    fn sealed_open_evaluation_draft_fixture(
        paper_id: Uuid,
        submission_id: Uuid,
        evaluation_id: Uuid,
        evaluator_player_id: Uuid,
    ) -> Value {
        let digest = format!("sha256:{}", "a".repeat(64));
        let public_key_bytes = [41_u8; 32];
        let public_key = BASE64.encode(public_key_bytes);
        let public_key_hash = sha256_digest(&public_key_bytes);
        let tolerance_policy = serde_json::json!({
            "schema":"hepta.paper_raid.tolerance_policy.v1",
            "version":"1",
            "rules":[{"kind":"absolute","metric":"accuracy","max_delta_micros":1000}]
        });
        let tolerance_policy_hash =
            sha256_digest(&canonical_json_bytes(&tolerance_policy).expect("canonical policy"));
        let mut draft = serde_json::json!({
            "schema":"hepta.paper_raid.evaluation_draft.v2",
            "evaluation_id":evaluation_id,
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "review_round":1,
            "supersedes_evaluation_id":null,
            "release_candidate_hash":digest,
            "paper_bundle_hash":digest,
            "tolerance_policy":tolerance_policy,
            "tolerance_policy_hash":tolerance_policy_hash,
            "reference_metrics_micros":{"accuracy":900000},
            "paper_score":{
                "schema":"hepta.paper_raid.paper_score.v1",
                "evaluation_id":evaluation_id,
                "paper_project_id":paper_id,
                "components":{},
                "hard_gates":{},
                "score_bps":0,
                "eligible":false,
                "score_hash":digest,
                "created_at":"2026-08-11T00:01:00Z"
            },
            "evaluator_player_id":evaluator_player_id,
            "evaluator_signing_key_id":"draft-evaluator-key",
            "evaluator_signing_public_key":public_key,
            "evaluator_signing_public_key_hash":public_key_hash,
            "evaluator_coi_attestation_hash":digest,
            "evaluator_signed_at_unix":1_786_406_460_i64,
            "evaluator_signature":BASE64.encode([42_u8;64]),
            "evaluation_signing_hash":digest,
            "draft_hash":digest,
            "status":"open",
            "version":1,
            "finalized_evaluation_id":null,
            "created_at":"2026-08-11T00:01:00Z",
            "expires_at":"2026-08-12T00:01:00Z",
            "updated_at":"2026-08-11T00:01:00Z",
            "finalized_at":null,
            "expired_at":null
        });
        draft["draft_hash"] = Value::String(
            sealed_evaluation_draft_hash(&draft, "hepta.paper_raid.evaluation_draft.v2")
                .expect("canonical draft hash"),
        );
        draft
    }

    struct SealedAssignmentFixture<'a> {
        assignment_id: Uuid,
        paper_id: Uuid,
        submission_id: Uuid,
        player_id: Uuid,
        slot: &'a str,
        pinned_evaluation_id: Option<Uuid>,
        status: &'a str,
        version: u64,
        updated_at: &'a str,
    }

    fn sealed_assignment_fixture(input: SealedAssignmentFixture<'_>) -> Value {
        let SealedAssignmentFixture {
            assignment_id,
            paper_id,
            submission_id,
            player_id,
            slot,
            pinned_evaluation_id,
            status,
            version,
            updated_at,
        } = input;
        serde_json::json!({
            "schema":"hepta.paper_raid.review_assignment.v1",
            "assignment_id":assignment_id,
            "paper_project_id":paper_id,
            "submission_id":submission_id,
            "player_id":player_id,
            "review_round":1,
            "slot":slot,
            "pinned_evaluation_id":pinned_evaluation_id,
            "status":status,
            "version":version,
            "claimed_at":"2026-08-11T00:00:00Z",
            "expires_at":"2026-08-11T02:00:00Z",
            "updated_at":updated_at
        })
    }

    #[test]
    fn sealed_review_state_accepts_open_and_expired_draft_pinned_panels() {
        let paper_id = Uuid::from_u128(1);
        let submission_id = Uuid::from_u128(2);
        let evaluation_id = Uuid::from_u128(3);
        let evaluator_id = Uuid::from_u128(4);
        let reviewer_id = Uuid::from_u128(5);
        let draft = sealed_open_evaluation_draft_fixture(
            paper_id,
            submission_id,
            evaluation_id,
            evaluator_id,
        );
        let assignments = vec![
            sealed_assignment_fixture(SealedAssignmentFixture {
                assignment_id: Uuid::from_u128(10),
                paper_id,
                submission_id,
                player_id: evaluator_id,
                slot: "evaluator",
                pinned_evaluation_id: Some(evaluation_id),
                status: "pinned",
                version: 2,
                updated_at: "2026-08-11T00:01:00Z",
            }),
            {
                let mut reviewer = sealed_assignment_fixture(SealedAssignmentFixture {
                    assignment_id: Uuid::from_u128(11),
                    paper_id,
                    submission_id,
                    player_id: reviewer_id,
                    slot: "reviewer_1",
                    pinned_evaluation_id: Some(evaluation_id),
                    status: "pinned",
                    version: 2,
                    updated_at: "2026-08-11T00:01:30Z",
                });
                // Reviewers may claim an open slot after the evaluator has
                // frozen the draft; their later attestation pins that lease.
                reviewer["claimed_at"] = serde_json::json!("2026-08-11T00:01:10Z");
                reviewer
            },
        ];
        AuthenticatedPaperReviewState::test_only_seal(
            paper_id,
            sealed_review_test_envelope(assignments.clone(), vec![draft.clone()]),
        )
        .expect("open draft pins its evaluator and attesting reviewer");

        let missing_draft = sealed_review_test_envelope(assignments.clone(), Vec::new());
        assert!(matches!(
            AuthenticatedPaperReviewState::test_only_seal(paper_id, missing_draft),
            Err(AppError::Upstream)
        ));

        let mut expired_draft = draft;
        expired_draft["status"] = serde_json::json!("expired");
        expired_draft["version"] = serde_json::json!(2);
        expired_draft["updated_at"] = expired_draft["expires_at"].clone();
        expired_draft["expired_at"] = expired_draft["expires_at"].clone();
        let expired_assignments = assignments
            .into_iter()
            .map(|mut assignment| {
                assignment["status"] = serde_json::json!("expired");
                assignment["version"] = serde_json::json!(3);
                assignment["updated_at"] = expired_draft["expires_at"].clone();
                assignment
            })
            .collect();
        AuthenticatedPaperReviewState::test_only_seal(
            paper_id,
            sealed_review_test_envelope(expired_assignments, vec![expired_draft]),
        )
        .expect("expired draft preserves its exact released pinned-panel lineage");
    }

    #[test]
    fn sealed_review_state_rejects_semantic_duplicate_live_assignments() {
        let paper_id = Uuid::from_u128(21);
        let submission_id = Uuid::from_u128(22);
        let evaluation_id = Uuid::from_u128(23);
        let evaluator_id = Uuid::from_u128(24);
        let reviewer_id = Uuid::from_u128(25);
        let draft = sealed_open_evaluation_draft_fixture(
            paper_id,
            submission_id,
            evaluation_id,
            evaluator_id,
        );
        let evaluator = sealed_assignment_fixture(SealedAssignmentFixture {
            assignment_id: Uuid::from_u128(30),
            paper_id,
            submission_id,
            player_id: evaluator_id,
            slot: "evaluator",
            pinned_evaluation_id: Some(evaluation_id),
            status: "pinned",
            version: 2,
            updated_at: "2026-08-11T00:01:00Z",
        });
        let reviewer = sealed_assignment_fixture(SealedAssignmentFixture {
            assignment_id: Uuid::from_u128(32),
            paper_id,
            submission_id,
            player_id: reviewer_id,
            slot: "reviewer_1",
            pinned_evaluation_id: Some(evaluation_id),
            status: "pinned",
            version: 2,
            updated_at: "2026-08-11T00:01:30Z",
        });

        let duplicate_slot = sealed_assignment_fixture(SealedAssignmentFixture {
            assignment_id: Uuid::from_u128(31),
            paper_id,
            submission_id,
            player_id: Uuid::from_u128(26),
            slot: "evaluator",
            pinned_evaluation_id: None,
            status: "claimed",
            version: 1,
            updated_at: "2026-08-11T00:00:00Z",
        });
        assert!(matches!(
            AuthenticatedPaperReviewState::test_only_seal(
                paper_id,
                sealed_review_test_envelope(
                    vec![evaluator.clone(), duplicate_slot, reviewer.clone()],
                    vec![draft.clone()],
                ),
            ),
            Err(AppError::Upstream)
        ));

        let duplicate_player = sealed_assignment_fixture(SealedAssignmentFixture {
            assignment_id: Uuid::from_u128(33),
            paper_id,
            submission_id,
            player_id: evaluator_id,
            slot: "reviewer_2",
            pinned_evaluation_id: None,
            status: "claimed",
            version: 1,
            updated_at: "2026-08-11T00:00:00Z",
        });
        assert!(matches!(
            AuthenticatedPaperReviewState::test_only_seal(
                paper_id,
                sealed_review_test_envelope(
                    vec![evaluator, reviewer, duplicate_player],
                    vec![draft],
                ),
            ),
            Err(AppError::Upstream)
        ));
    }

    #[test]
    fn appeal_player_frames_derive_one_exact_authoritative_lineage() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let appeal_id = Uuid::new_v4();
        let first_panel = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let second_panel = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let release = format!("sha256:{}", "a".repeat(64));
        let bundle = format!("sha256:{}", "b".repeat(64));
        let evaluation =
            |evaluation_id: Uuid, version: u64, supersedes: Option<Uuid>, panel: [Uuid; 3]| {
                serde_json::json!({
                    "evaluation_id":evaluation_id,
                    "paper_project_id":paper_id,
                    "submission_id":submission_id,
                    "release_candidate_hash":release,
                    "paper_bundle_hash":bundle,
                    "version":version,
                    "supersedes_evaluation_id":supersedes,
                    "evaluator_player_id":panel[0],
                    "reviewer_attestations":[
                        {"reviewer_player_id":panel[1]},
                        {"reviewer_player_id":panel[2]}
                    ]
                })
            };
        let review = serde_json::json!({
            "evaluations":[
                evaluation(first_id, 1, None, first_panel),
                evaluation(second_id, 2, Some(first_id), second_panel)
            ],
            "appeals":[{
                "appeal_id":appeal_id,
                "evaluation_id":first_id,
                "paper_project_id":paper_id,
                "release_candidate_hash":release,
                "appellant_player_id":Uuid::new_v4()
            }],
            "resolutions":[]
        });
        assert_eq!(
            required_uuid(
                latest_paper_evaluation(&review, paper_id).expect("latest evaluation"),
                "evaluation_id"
            )
            .expect("evaluation ID"),
            second_id
        );
        let appealed =
            paper_evaluation_by_id(&review, paper_id, first_id).expect("appealed evaluation");
        assert_eq!(
            required_uuid(
                exact_open_appeal(&review, paper_id, first_id).expect("one open Appeal"),
                "appeal_id"
            )
            .expect("Appeal ID"),
            appeal_id
        );
        assert_eq!(
            required_uuid(
                exact_superseding_evaluation(&review, appealed)
                    .expect("valid lineage")
                    .expect("superseding evaluation"),
                "evaluation_id"
            )
            .expect("superseding ID"),
            second_id
        );
        assert!(evaluation_panel_contains(appealed, first_panel[0]).expect("panel"));
        assert!(!evaluation_panel_contains(appealed, Uuid::new_v4()).expect("independent"));

        let mut ambiguous = review.clone();
        ambiguous["evaluations"]
            .as_array_mut()
            .expect("evaluations")
            .push(evaluation(
                Uuid::new_v4(),
                2,
                Some(first_id),
                [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            ));
        assert!(matches!(
            exact_superseding_evaluation(&ambiguous, appealed),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn paper_rework_adapter_selects_current_submission_and_monotonic_cycle() {
        let paper_id = Uuid::new_v4();
        let submission_id = Uuid::new_v4();
        let old_submission_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let review = serde_json::json!({
            "evaluations":[
                {
                    "evaluation_id":Uuid::new_v4(),
                    "paper_project_id":paper_id,
                    "submission_id":old_submission_id,
                    "version":1,
                    "status":"rejected",
                    "supersedes_evaluation_id":null
                },
                {
                    "evaluation_id":evaluation_id,
                    "paper_project_id":paper_id,
                    "submission_id":submission_id,
                    "version":1,
                    "status":"rejected",
                    "supersedes_evaluation_id":null
                }
            ]
        });
        assert_eq!(
            required_uuid(
                latest_submission_evaluation(&review, paper_id, submission_id)
                    .expect("current submission evaluation"),
                "evaluation_id",
            )
            .expect("evaluation ID"),
            evaluation_id,
        );

        let rework_id_2 = Uuid::new_v4();
        let rework_id_3 = Uuid::new_v4();
        let completed = |rework_id: Uuid, cycle: u64| {
            let rejected_commitment =
                format!("sha256:{}", if cycle == 2 { "1" } else { "3" }.repeat(64));
            let replacement_commitment =
                format!("sha256:{}", if cycle == 2 { "2" } else { "4" }.repeat(64));
            serde_json::json!({
                "rework":{
                    "schema":"hepta.paper_raid.rework_record.v1",
                    "rework_id":rework_id,
                    "paper_project_id":paper_id,
                    "rework_cycle":cycle,
                    "rejected_rework_content_commitment_sha256":rejected_commitment
                },
                "resubmission":{
                    "schema":"hepta.paper_raid.rework_resubmission.v1",
                    "rework_id":rework_id,
                    "paper_project_id":paper_id,
                    "replacement_review_round":1,
                    "replacement_rework_content_commitment_sha256":replacement_commitment
                }
            })
        };
        let history = serde_json::json!([completed(rework_id_2, 2), completed(rework_id_3, 3)]);
        assert_eq!(
            next_paper_rework_cycle(&history, paper_id).expect("next cycle"),
            4
        );

        let mut active = history.clone();
        active[1]["resubmission"] = Value::Null;
        assert!(matches!(
            next_paper_rework_cycle(&active, paper_id),
            Err(AppError::Conflict(_))
        ));
        let gap = serde_json::json!([completed(rework_id_3, 3)]);
        assert!(matches!(
            next_paper_rework_cycle(&gap, paper_id),
            Err(AppError::Upstream)
        ));

        let mut identity_only = history.clone();
        identity_only[1]["resubmission"]["replacement_rework_content_commitment_sha256"] =
            identity_only[1]["rework"]["rejected_rework_content_commitment_sha256"].clone();
        assert!(matches!(
            next_paper_rework_cycle(&identity_only, paper_id),
            Err(AppError::Upstream)
        ));
    }

    #[test]
    fn appeal_evidence_selector_resolves_only_one_paper_manifest() {
        let paper_id = Uuid::new_v4();
        let manifest_id = Uuid::new_v4();
        let manifest_hash = format!("sha256:{}", "c".repeat(64));
        let room = serde_json::json!({
            "artifact_manifests":[{
                "manifest_id":manifest_id,
                "paper_project_id":paper_id,
                "manifest_hash":manifest_hash
            }]
        });
        assert_eq!(
            exact_manifest_hash(&room, paper_id, Some(manifest_id), None)
                .expect("selected manifest"),
            manifest_hash
        );
        assert!(matches!(
            exact_manifest_hash(&room, Uuid::new_v4(), Some(manifest_id), None),
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            exact_manifest_hash(&room, paper_id, Some(Uuid::new_v4()), None),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn human_registration_is_server_derived_and_exactly_signed() {
        let identity = AlphaIdentity::test_identity(
            "subject-alpha",
            Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("player"),
            Uuid::parse_str("22222222-2222-4222-8222-222222222222").expect("nakama"),
        );
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
        let first =
            HeptaClient::human_registration_challenge(&identity, &public_key, 1_800_000_123)
                .expect("challenge");
        let replay =
            HeptaClient::human_registration_challenge(&identity, &public_key, 1_800_000_299)
                .expect("same time bucket");
        assert_eq!(first.idempotency_key, replay.idempotency_key);
        assert_eq!(first.signing_bytes, replay.signing_bytes);
        assert_eq!(first.player_id, identity.player_id);
        assert_eq!(first.subject_id, identity.subject_id);
        assert_eq!(first.nakama_user_id, identity.nakama_user_id);
        assert_eq!(first.display_name, identity.display_name);
        assert_eq!(first.issued_at_unix, 1_800_000_000);
        assert_eq!(first.expires_at_unix, 1_800_000_600);
        assert!(first.signing_key_id.starts_with("human-ed25519:"));

        let bytes = BASE64.decode(&first.signing_bytes).expect("signing bytes");
        let signature = BASE64.encode(signing_key.sign(&bytes).to_bytes());
        verify_human_key_registration_pop(&human_registration_claim(&first), &signature)
            .expect("valid proof");

        let mut tampered = human_registration_claim(&first);
        tampered.subject_id = "attacker".into();
        assert!(verify_human_key_registration_pop(&tampered, &signature).is_err());
        assert!(build_human_registration_challenge(
            &identity,
            &public_key,
            first.issued_at_unix,
            first.expires_at_unix,
        )
        .is_err());
    }

    #[test]
    fn committed_registration_response_loss_recovers_only_the_same_imported_key() {
        let identity = AlphaIdentity::test_identity(
            "subject-recovery",
            Uuid::parse_str("31111111-1111-4111-8111-111111111111").expect("player"),
            Uuid::parse_str("32222222-2222-4222-8222-222222222222").expect("nakama"),
        );
        let signing_key = SigningKey::from_bytes(&[17_u8; 32]);
        let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
        let challenge =
            HeptaClient::human_registration_challenge(&identity, &public_key, 1_800_000_123)
                .expect("challenge");
        let signature = BASE64.encode(
            signing_key
                .sign(
                    &BASE64
                        .decode(&challenge.signing_bytes)
                        .expect("signing bytes"),
                )
                .to_bytes(),
        );
        let proof = HumanRegistrationProof {
            signing_public_key: public_key,
            idempotency_key: challenge.idempotency_key,
            issued_at_unix: challenge.issued_at_unix,
            expires_at_unix: challenge.expires_at_unix,
            key_proof_signature: signature,
        };

        let original = registration_challenge_from_proof(&identity, &proof, proof.issued_at_unix)
            .expect("original proof remains verifiable for an applied self-read recovery");
        let committed_player = serde_json::json!({
            "player_id": identity.player_id,
            "subject_id": identity.subject_id,
            "signing_key_id": original.signing_key_id,
            "signing_public_key": original.signing_public_key,
            "signing_public_key_hash": original.signing_public_key_hash,
        });
        validate_recovered_human_player(&identity, &original, &committed_player)
            .expect("same imported key recovers committed registration");

        let mut different = committed_player;
        different["signing_public_key"] = Value::String(BASE64.encode([23_u8; 32]));
        assert!(matches!(
            validate_recovered_human_player(&identity, &original, &different),
            Err(AppError::Conflict(_))
        ));
        assert!(
            registration_challenge_from_proof(&identity, &proof, proof.expires_at_unix,).is_err()
        );
    }

    #[test]
    fn whitelist_builds_exact_p1_route() {
        let paper_id = Uuid::new_v4();
        let command = BrowserCommand {
            command: CommandName::CreatePaperRevision,
            resource_id: Some(paper_id),
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: serde_json::json!({"human_signature": "browser-owned"}),
        };
        let route = command.route().expect("route");
        assert_eq!(route.path, format!("/v2/hepta/papers/{paper_id}/revisions"));
        assert_eq!(route.operation, "create_paper_revision_v2");
    }

    #[test]
    fn matchmaking_roles_are_restricted_to_identity_authorizations() {
        let mut identity =
            AlphaIdentity::test_identity("subject-role", Uuid::new_v4(), Uuid::new_v4());
        identity.author_roles = vec![AlphaAuthorRole::Evidence, AlphaAuthorRole::Experiment].into();
        validate_identity_command_payload(
            &identity,
            CommandName::QueueMatchmaking,
            &serde_json::json!({"roles":["evidence","experiment"]}),
        )
        .expect("authorized preferences");
        assert!(matches!(
            validate_identity_command_payload(
                &identity,
                CommandName::QueueMatchmaking,
                &serde_json::json!({"roles":["captain"]}),
            ),
            Err(AppError::Forbidden)
        ));
        assert!(matches!(
            validate_identity_command_payload(
                &identity,
                CommandName::QueueMatchmaking,
                &serde_json::json!({"roles":["evidence","evidence"]}),
            ),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn review_claim_is_bound_to_identity_and_exact_scope() {
        let mut reviewer =
            AlphaIdentity::test_identity("subject-reviewer", Uuid::new_v4(), Uuid::new_v4());
        reviewer.scopes = vec![AlphaIdentityScope::Reviewer].into();
        reviewer.author_roles = Vec::new().into();
        let payload = serde_json::json!({
            "assignment_id": Uuid::new_v4(),
            "player_id": reviewer.player_id,
            "review_round": 1,
            "slot": "reviewer_2",
        });
        validate_identity_command_payload(&reviewer, CommandName::ClaimReviewAssignment, &payload)
            .expect("reviewer may claim reviewer slot");

        let mut wrong_player = payload.clone();
        wrong_player["player_id"] = Value::String(Uuid::new_v4().to_string());
        assert!(matches!(
            validate_identity_command_payload(
                &reviewer,
                CommandName::ClaimReviewAssignment,
                &wrong_player,
            ),
            Err(AppError::Forbidden)
        ));

        let mut wrong_scope = payload;
        wrong_scope["slot"] = Value::String("evaluator".into());
        assert!(matches!(
            validate_identity_command_payload(
                &reviewer,
                CommandName::ClaimReviewAssignment,
                &wrong_scope,
            ),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn appeal_actor_fields_are_bound_to_the_authenticated_identity() {
        let author = AlphaIdentity::test_identity("author", Uuid::new_v4(), Uuid::new_v4());
        validate_identity_command_payload(
            &author,
            CommandName::StartPaperRework,
            &serde_json::json!({"author_player_id":author.player_id}),
        )
        .expect("author identity matches Paper rework payload");
        assert!(matches!(
            validate_identity_command_payload(
                &author,
                CommandName::StartPaperRework,
                &serde_json::json!({"author_player_id":Uuid::new_v4()}),
            ),
            Err(AppError::Forbidden)
        ));
        validate_identity_command_payload(
            &author,
            CommandName::SubmitAppeal,
            &serde_json::json!({"appellant_player_id":author.player_id}),
        )
        .expect("author identity matches Appeal payload");
        assert!(matches!(
            validate_identity_command_payload(
                &author,
                CommandName::SubmitAppeal,
                &serde_json::json!({"appellant_player_id":Uuid::new_v4()}),
            ),
            Err(AppError::Forbidden)
        ));

        let mut resolver = AlphaIdentity::test_identity("resolver", Uuid::new_v4(), Uuid::new_v4());
        resolver.scopes = vec![AlphaIdentityScope::Reproducer].into();
        resolver.author_roles = Vec::new().into();
        validate_identity_command_payload(
            &resolver,
            CommandName::ResolveAppeal,
            &serde_json::json!({"resolver_player_id":resolver.player_id}),
        )
        .expect("resolver identity matches resolution payload");
        assert!(matches!(
            validate_identity_command_payload(
                &author,
                CommandName::ResolveAppeal,
                &serde_json::json!({"resolver_player_id":author.player_id}),
            ),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn p5_child_routes_fail_closed_without_exact_child() {
        let missing_child = BrowserCommand {
            command: CommandName::SubmitReproduction,
            resource_id: Some(Uuid::new_v4()),
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: Value::Null,
        };
        assert!(matches!(missing_child.route(), Err(AppError::Invalid(_))));

        for name in [
            CommandName::SubmitEvaluationDraftAttestation,
            CommandName::FinalizePaperEvaluationDraft,
            CommandName::SubmitReproduction,
        ] {
            assert!(command(name, Some(Uuid::new_v4()), None, None)
                .route()
                .is_err());
            assert!(command(
                name,
                Some(Uuid::new_v4()),
                Some(Uuid::new_v4()),
                Some("forbidden.session")
            )
            .route()
            .is_err());
        }
        assert!(command(
            CommandName::CreatePaperEvaluationDraft,
            Some(Uuid::new_v4()),
            Some(Uuid::new_v4()),
            None,
        )
        .route()
        .is_err());
    }

    #[test]
    fn join_rejects_path_confusion() {
        let base = Url::parse("http://127.0.0.1:9000").expect("base");
        assert!(join_exact(&base, "/v2/hepta/papers/../admin").is_err());
        assert_eq!(
            join_exact(&base, "/v2/hepta/papers")
                .expect("joined")
                .as_str(),
            "http://127.0.0.1:9000/v2/hepta/papers"
        );
    }

    fn command(
        name: CommandName,
        resource_id: Option<Uuid>,
        child_id: Option<Uuid>,
        session_id: Option<&str>,
    ) -> BrowserCommand {
        BrowserCommand {
            command: name,
            resource_id,
            child_id,
            session_id: session_id.map(str::to_string),
            idempotency_key: Uuid::new_v4(),
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn exact_paths_match_committed_openapi() {
        let team = Uuid::new_v4();
        let paper = Uuid::new_v4();
        let work_item = Uuid::new_v4();
        let revision = Uuid::new_v4();
        let binding = Uuid::new_v4();
        let cases = [
            (
                command(CommandName::CreateAgentBinding, None, None, None),
                "/v2/hepta/agent-bindings".into(),
                "create_agent_binding_v2",
            ),
            (
                command(
                    CommandName::RotateAgentBindingKey,
                    Some(binding),
                    None,
                    None,
                ),
                format!("/v2/hepta/agent-bindings/{binding}/rotate-key"),
                "rotate_agent_binding_key_v2",
            ),
            (
                command(
                    CommandName::AcceptResearchTeamMembership,
                    Some(team),
                    None,
                    None,
                ),
                format!("/v2/hepta/teams/{team}/member-acceptances"),
                "accept_research_team_membership_v2",
            ),
            (
                command(
                    CommandName::TransitionPaperChallengeOutcome,
                    Some(paper),
                    None,
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/outcome"),
                "transition_paper_challenge_outcome_v1",
            ),
            (
                command(
                    CommandName::TransitionPaperWorkItem,
                    Some(work_item),
                    None,
                    None,
                ),
                format!("/v2/hepta/work-items/{work_item}/transition"),
                "transition_paper_work_item_v2",
            ),
            (
                command(
                    CommandName::PromotePaperReleaseCandidate,
                    Some(paper),
                    Some(revision),
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/revisions/{revision}/promote"),
                "promote_paper_release_candidate_v2",
            ),
            (
                command(
                    CommandName::IssueResearchSessionAuthorizationSet,
                    None,
                    None,
                    None,
                ),
                "/v2/hepta/research-session-authorizations".into(),
                "issue_research_session_authorization_set_v1",
            ),
            (
                command(
                    CommandName::ReplaceResearchSessionAuthorizationSet,
                    None,
                    None,
                    Some("paper.raid:alpha"),
                ),
                "/v2/hepta/research-session-authorizations/paper.raid:alpha/replace".into(),
                "replace_research_session_authorization_set_v1",
            ),
            (
                command(
                    CommandName::CreateNakamaResearchSessionControl,
                    None,
                    None,
                    None,
                ),
                "/v2/hepta/nakama/research-session-controls/create".into(),
                "create_nakama_research_session_control_v2",
            ),
            (
                command(
                    CommandName::ResumeNakamaResearchSessionControl,
                    None,
                    None,
                    None,
                ),
                "/v2/hepta/nakama/research-session-controls/resume".into(),
                "resume_nakama_research_session_control_v2",
            ),
            (
                command(
                    CommandName::ReplaceNakamaResearchSessionRosterControl,
                    None,
                    None,
                    None,
                ),
                "/v2/hepta/nakama/research-session-controls/replace-roster".into(),
                "replace_nakama_research_session_roster_control_v2",
            ),
            (
                command(
                    CommandName::CompleteNakamaResearchSessionControl,
                    None,
                    None,
                    None,
                ),
                "/v2/hepta/nakama/research-session-controls/complete".into(),
                "complete_nakama_research_session_control_v2",
            ),
            (
                command(CommandName::QueueMatchmaking, None, None, None),
                "/v2/hepta/matchmaking/tickets".into(),
                "create_matchmaking_ticket_v3",
            ),
            (
                command(CommandName::CancelMatchmakingTicket, Some(team), None, None),
                format!("/v2/hepta/matchmaking/tickets/{team}/cancel"),
                "cancel_matchmaking_ticket_v3",
            ),
            (
                command(CommandName::DecideTeamProposal, Some(team), None, None),
                format!("/v2/hepta/team-proposals/{team}/decisions"),
                "create_team_proposal_decision_v3",
            ),
            (
                command(CommandName::MaterializeTeamProposal, Some(team), None, None),
                format!("/v2/hepta/team-proposals/{team}/materialize"),
                "materialize_team_proposal_v1",
            ),
            (
                command(CommandName::RegisterArtifact, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/artifact-manifests"),
                "create_artifact_manifest_v3",
            ),
            (
                command(CommandName::SubmitReview, Some(paper), Some(revision), None),
                format!("/v2/hepta/papers/{paper}/section-revisions/{revision}/reviews"),
                "create_section_review_v3",
            ),
            (
                command(CommandName::MergeSection, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/section-merges"),
                "create_section_merge_v3",
            ),
            (
                command(
                    CommandName::CreateContributionLedger,
                    Some(paper),
                    None,
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/contribution-ledgers"),
                "create_contribution_ledger_v1",
            ),
            (
                command(CommandName::ClaimReviewAssignment, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/review-assignments"),
                "claim_paper_review_assignment_v1",
            ),
            (
                command(CommandName::CreatePaperEvaluation, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/evaluations"),
                "create_paper_evaluation_v1",
            ),
            (
                command(
                    CommandName::CreatePaperEvaluationDraft,
                    Some(paper),
                    None,
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/evaluation-drafts"),
                "create_paper_evaluation_draft_v1",
            ),
            (
                command(
                    CommandName::SubmitEvaluationDraftAttestation,
                    Some(paper),
                    Some(revision),
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/evaluation-drafts/{revision}/attestations"),
                "submit_paper_evaluation_draft_attestation_v1",
            ),
            (
                command(
                    CommandName::FinalizePaperEvaluationDraft,
                    Some(paper),
                    Some(revision),
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/evaluation-drafts/{revision}/finalize"),
                "finalize_paper_evaluation_draft_v1",
            ),
            (
                command(
                    CommandName::SubmitReproduction,
                    Some(paper),
                    Some(revision),
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/evaluations/{revision}/reproductions"),
                "create_paper_reproduction_v1",
            ),
            (
                command(CommandName::StartPaperRework, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/reworks"),
                "start_paper_rework_v1",
            ),
            (
                command(CommandName::SubmitAppeal, Some(paper), Some(revision), None),
                format!("/v2/hepta/papers/{paper}/evaluations/{revision}/appeals"),
                "create_paper_appeal_v1",
            ),
            (
                command(
                    CommandName::ResolveAppeal,
                    Some(paper),
                    Some(revision),
                    None,
                ),
                format!("/v2/hepta/papers/{paper}/appeals/{revision}/resolve"),
                "resolve_paper_appeal_v1",
            ),
        ];
        for (command, expected_path, expected_operation) in cases {
            let route = command.route().expect("exact route");
            assert_eq!(route.method, Method::POST);
            assert_eq!(route.path, expected_path);
            assert_eq!(route.operation, expected_operation);
        }
    }

    #[test]
    fn browser_command_rejects_caller_supplied_path_and_operation() {
        let idempotency_key = Uuid::new_v4();
        let base = serde_json::json!({
            "command": "create_nakama_research_session_control",
            "resource_id": null,
            "child_id": null,
            "session_id": null,
            "idempotency_key": idempotency_key,
            "payload": {
                "authorization_set_id": Uuid::new_v4(),
                "idempotency_key": idempotency_key
            }
        });
        assert!(serde_json::from_value::<BrowserCommand>(base.clone()).is_ok());

        let mut with_path = base.clone();
        with_path
            .as_object_mut()
            .expect("command object")
            .insert("path".into(), Value::String("/v2/hepta/admin".into()));
        assert!(serde_json::from_value::<BrowserCommand>(with_path).is_err());

        let mut with_operation = base;
        with_operation
            .as_object_mut()
            .expect("command object")
            .insert("operation".into(), Value::String("forged_v2".into()));
        assert!(serde_json::from_value::<BrowserCommand>(with_operation).is_err());
    }

    #[test]
    fn nakama_control_routes_reject_all_unused_envelope_locators() {
        for name in [
            CommandName::CreateNakamaResearchSessionControl,
            CommandName::ResumeNakamaResearchSessionControl,
            CommandName::ReplaceNakamaResearchSessionRosterControl,
            CommandName::CompleteNakamaResearchSessionControl,
        ] {
            assert!(command(name, Some(Uuid::new_v4()), None, None)
                .route()
                .is_err());
            assert!(command(name, None, Some(Uuid::new_v4()), None)
                .route()
                .is_err());
            assert!(command(name, None, None, Some("paper.raid:alpha"))
                .route()
                .is_err());
        }
    }

    #[test]
    fn payload_idempotency_is_injected_or_must_match() {
        let request = command(CommandName::CreateResearchTeam, None, None, None);
        let payload = request.canonical_payload().expect("injected payload");
        let expected = request.idempotency_key.to_string();
        assert_eq!(
            payload.get("idempotency_key").and_then(Value::as_str),
            Some(expected.as_str())
        );
        let mut bad = command(CommandName::CreateResearchTeam, None, None, None);
        bad.payload = serde_json::json!({"idempotency_key": Uuid::new_v4()});
        assert!(matches!(
            bad.canonical_payload(),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn matchmaking_party_payload_accepts_only_the_canonical_digest_field() {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert!(matchmaking_party_payload_is_safe(&serde_json::json!({
            "roles":["captain"],
            "party_code_hash":digest,
        })));
        assert!(matchmaking_party_payload_is_safe(&serde_json::json!({
            "roles":["captain"],
        })));
        for unsafe_payload in [
            serde_json::json!({"roles":["captain"],"party_code":"PR1-raw"}),
            serde_json::json!({"roles":["captain"],"raw_party_code":"PR1-raw"}),
            serde_json::json!({"roles":["captain"],"party_code_hash":format!("sha256:{}", "A".repeat(64))}),
            serde_json::json!({"roles":["captain"],"party_code_hash":"not-a-digest"}),
            serde_json::json!({"roles":["captain"],"party_code_hash":null}),
        ] {
            assert!(!matchmaking_party_payload_is_safe(&unsafe_payload));
        }
    }

    #[derive(Default)]
    struct MockHeptaState {
        calls: usize,
        bodies: Vec<Vec<u8>>,
        assertions: Vec<SignedConsumerUserAssertionV2>,
        event_queries: Vec<String>,
    }

    async fn mock_create_team(
        State((captured, verifying_key)): State<(
            Arc<Mutex<MockHeptaState>>,
            ed25519_dalek::VerifyingKey,
        )>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let assertion = headers
            .get(ASSERTION_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| serde_json::from_slice::<SignedConsumerUserAssertionV2>(&bytes).ok());
        let Some(assertion) = assertion else {
            return AxumStatus::BAD_REQUEST.into_response();
        };
        if verify_consumer_user_assertion_signature(&assertion, &verifying_key).is_err()
            || assertion.claim.canonical_path != "/v2/hepta/teams"
            || assertion.claim.operation != "create_research_team_v2"
            || assertion.claim.body_hash != sha256_digest(&body)
        {
            return AxumStatus::BAD_REQUEST.into_response();
        }
        let body_json: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return AxumStatus::BAD_REQUEST.into_response(),
        };
        if body_json.get("idempotency_key").and_then(Value::as_str)
            != Some(assertion.claim.idempotency_key.as_str())
        {
            return AxumStatus::BAD_REQUEST.into_response();
        }
        let mut captured = captured.lock().expect("mock lock");
        captured.calls += 1;
        captured.bodies.push(body.to_vec());
        captured.assertions.push(assertion);
        (
            AxumStatus::CREATED,
            Json(serde_json::json!({"team_id":"00000000-0000-0000-0000-000000000123","version":1})),
        )
            .into_response()
    }

    async fn mock_paper_events(
        State((captured, verifying_key)): State<(
            Arc<Mutex<MockHeptaState>>,
            ed25519_dalek::VerifyingKey,
        )>,
        Path(paper_id): Path<Uuid>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
    ) -> Response {
        let assertion = headers
            .get(ASSERTION_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| serde_json::from_slice::<SignedConsumerUserAssertionV2>(&bytes).ok());
        let Some(assertion) = assertion else {
            return AxumStatus::BAD_REQUEST.into_response();
        };
        let after_cursor = query.get("after_cursor").cloned().unwrap_or_default();
        let expected_path =
            format!("/v2/hepta/papers/{paper_id}/events?after_cursor={after_cursor}");
        if verify_consumer_user_assertion_signature(&assertion, &verifying_key).is_err()
            || assertion.claim.canonical_path != expected_path
            || assertion.claim.operation != "list_paper_room_events_v3"
            || assertion.claim.body_hash != sha256_digest(&[])
        {
            return AxumStatus::BAD_REQUEST.into_response();
        }
        let mut captured = captured.lock().expect("mock lock");
        captured.event_queries.push(expected_path);
        captured.assertions.push(assertion);
        Json(serde_json::json!([{"cursor":42}])).into_response()
    }

    async fn mock_nakama_control(
        State((captured, verifying_key)): State<(
            Arc<Mutex<MockHeptaState>>,
            ed25519_dalek::VerifyingKey,
        )>,
        OriginalUri(uri): OriginalUri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let expected_operation = match uri.path() {
            "/v2/hepta/nakama/research-session-controls/create" => {
                "create_nakama_research_session_control_v2"
            }
            "/v2/hepta/nakama/research-session-controls/resume" => {
                "resume_nakama_research_session_control_v2"
            }
            "/v2/hepta/nakama/research-session-controls/replace-roster" => {
                "replace_nakama_research_session_roster_control_v2"
            }
            "/v2/hepta/nakama/research-session-controls/complete" => {
                "complete_nakama_research_session_control_v2"
            }
            _ => return AxumStatus::NOT_FOUND.into_response(),
        };
        let assertion = headers
            .get(ASSERTION_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| BASE64.decode(value).ok())
            .and_then(|bytes| serde_json::from_slice::<SignedConsumerUserAssertionV2>(&bytes).ok());
        let Some(assertion) = assertion else {
            return AxumStatus::BAD_REQUEST.into_response();
        };
        let body_json: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => return AxumStatus::BAD_REQUEST.into_response(),
        };
        if verify_consumer_user_assertion_signature(&assertion, &verifying_key).is_err()
            || assertion.claim.http_method != "POST"
            || assertion.claim.canonical_path != uri.path()
            || assertion.claim.operation != expected_operation
            || assertion.claim.body_hash != sha256_digest(&body)
            || body_json.get("idempotency_key").and_then(Value::as_str)
                != Some(assertion.claim.idempotency_key.as_str())
            || body_json.get("path").is_some()
            || body_json.get("operation").is_some()
        {
            return AxumStatus::BAD_REQUEST.into_response();
        }
        let mut captured = captured.lock().expect("mock lock");
        captured.calls += 1;
        captured.bodies.push(body.to_vec());
        captured.assertions.push(assertion);
        (
            AxumStatus::CREATED,
            Json(serde_json::json!({
                "schema":"hepta.paper_raid.nakama_research_control_command.v2",
                "status":"pending"
            })),
        )
            .into_response()
    }

    async fn spawn_mock_hepta(
        captured: Arc<Mutex<MockHeptaState>>,
        verifying_key: ed25519_dalek::VerifyingKey,
    ) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind Hepta mock");
        let address = listener.local_addr().expect("mock address");
        let router = Router::new()
            .route("/v2/hepta/teams", post(mock_create_team))
            .route("/v2/hepta/papers/:paper_id/events", get(mock_paper_events))
            .route(
                "/v2/hepta/nakama/research-session-controls/create",
                post(mock_nakama_control),
            )
            .route(
                "/v2/hepta/nakama/research-session-controls/resume",
                post(mock_nakama_control),
            )
            .route(
                "/v2/hepta/nakama/research-session-controls/replace-roster",
                post(mock_nakama_control),
            )
            .route(
                "/v2/hepta/nakama/research-session-controls/complete",
                post(mock_nakama_control),
            )
            .with_state((captured, verifying_key));
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("serve Hepta mock");
        });
        Url::parse(&format!("http://{address}")).expect("mock URL")
    }

    #[tokio::test]
    async fn real_postgres_pending_retry_restart_exact_cache_and_assertion_tamper() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!("PAPER_RAID_BFF_TEST_DATABASE_URL is unset; idempotency gate skipped");
            return;
        };
        let pool = crate::db::connect(&database_url)
            .await
            .expect("connect test PostgreSQL");
        crate::db::migrate(&pool)
            .await
            .expect("migrate test PostgreSQL");
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&[31_u8; 32]);
        let assertions = ConsumerAssertionConfig {
            issuer: "consumer-edge-test".into(),
            audience: "hepta-test".into(),
            key_id: "consumer-key-test".into(),
            signing_key: Arc::new(signing_key.clone()),
            ttl: Duration::from_secs(30),
        };
        let captured = Arc::new(Mutex::new(MockHeptaState::default()));
        let base = spawn_mock_hepta(captured.clone(), signing_key.verifying_key()).await;
        let identity = AlphaIdentity::test_identity(
            &format!("idempotency-test-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let request = BrowserCommand {
            command: CommandName::CreateResearchTeam,
            resource_id: None,
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: serde_json::json!({"challenge_id":"alpha-challenge"}),
        };
        let route = request.route().expect("route");
        let payload = request.canonical_payload().expect("canonical payload");
        let body = canonical_json_bytes(&payload).expect("canonical body");
        let request_hash = business_request_hash(&route, &body);

        let first_process = HeptaClient::new(
            base.clone(),
            assertions.clone(),
            pool.clone(),
            Metrics::default(),
        )
        .expect("first client");
        assert!(first_process
            .begin_idempotent(&identity, request.idempotency_key, &request_hash)
            .await
            .expect("seed pending request")
            .is_none());
        drop(first_process);

        let restarted =
            HeptaClient::new(base, assertions, pool, Metrics::default()).expect("restarted client");
        let response = restarted
            .forward_command(&identity, &request)
            .await
            .expect("pending request safely retries");
        assert_eq!(response.status, 201);
        assert!(!response.replayed);
        let exact_body = response.body.clone();
        let replay = restarted
            .forward_command(&identity, &request)
            .await
            .expect("completed response replays");
        assert_eq!(replay.status, 201);
        assert!(replay.replayed);
        assert_eq!(replay.body, exact_body);

        let changed = BrowserCommand {
            command: CommandName::CreateResearchTeam,
            resource_id: None,
            child_id: None,
            session_id: None,
            idempotency_key: request.idempotency_key,
            payload: serde_json::json!({"challenge_id":"different"}),
        };
        assert!(matches!(
            restarted.forward_command(&identity, &changed).await,
            Err(AppError::Conflict(_))
        ));

        for (name, payload) in [
            (
                CommandName::CreateNakamaResearchSessionControl,
                serde_json::json!({"authorization_set_id": Uuid::new_v4()}),
            ),
            (
                CommandName::ResumeNakamaResearchSessionControl,
                serde_json::json!({"session_id":"paper.raid:alpha","roster_version":1}),
            ),
            (
                CommandName::ReplaceNakamaResearchSessionRosterControl,
                serde_json::json!({"authorization_set_id": Uuid::new_v4()}),
            ),
            (
                CommandName::CompleteNakamaResearchSessionControl,
                serde_json::json!({"session_id":"paper.raid:alpha","roster_version":1}),
            ),
        ] {
            let control = BrowserCommand {
                command: name,
                resource_id: None,
                child_id: None,
                session_id: None,
                idempotency_key: Uuid::new_v4(),
                payload,
            };
            let response = restarted
                .forward_command(&identity, &control)
                .await
                .expect("fixed Nakama control route");
            assert_eq!(response.status, 201);
        }

        let paper_id = Uuid::new_v4();
        let events = restarted
            .list_paper_room_events(&identity, paper_id, 42)
            .await
            .expect("query-bound Paper Room events");
        assert_eq!(events[0]["cursor"], 42);

        let captured = captured.lock().expect("mock lock");
        assert_eq!(captured.calls, 5);
        assert_eq!(captured.bodies.len(), 5);
        assert_eq!(captured.bodies[0], body);
        assert_eq!(
            captured.assertions[1..5]
                .iter()
                .map(|assertion| (
                    assertion.claim.canonical_path.as_str(),
                    assertion.claim.operation.as_str(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "/v2/hepta/nakama/research-session-controls/create",
                    "create_nakama_research_session_control_v2",
                ),
                (
                    "/v2/hepta/nakama/research-session-controls/resume",
                    "resume_nakama_research_session_control_v2",
                ),
                (
                    "/v2/hepta/nakama/research-session-controls/replace-roster",
                    "replace_nakama_research_session_roster_control_v2",
                ),
                (
                    "/v2/hepta/nakama/research-session-controls/complete",
                    "complete_nakama_research_session_control_v2",
                ),
            ]
        );
        assert_eq!(
            captured.event_queries,
            vec![format!(
                "/v2/hepta/papers/{paper_id}/events?after_cursor=42"
            )]
        );
        let mut tampered = captured.assertions[0].clone();
        tampered.claim.operation = "tampered_operation".into();
        assert!(
            verify_consumer_user_assertion_signature(&tampered, &signing_key.verifying_key())
                .is_err()
        );
    }
}
