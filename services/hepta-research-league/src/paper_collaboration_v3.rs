use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use super::*;
use crate::paper_raid_contracts::{
    agent_proposal_signing_bytes, human_decision_signing_bytes,
    human_evidence_verification_signing_bytes, section_merge_signing_bytes,
    section_review_signing_bytes, AgentProposalSigningV1, HumanDecisionSigningV1,
    HumanEvidenceVerificationSigningV1, SectionMergeSigningV1, SectionReviewSigningV1,
    AGENT_PROPOSAL_V1, HUMAN_DECISION_V1, HUMAN_EVIDENCE_VERIFICATION_V1, SECTION_MERGE_V1,
    SECTION_REVIEW_V1,
};

pub const PAPER_COLLABORATION_PROTOCOL_V3: &str = "hepta.paper_raid.collaboration.v3";
pub const SOURCE_ARTIFACT_BUNDLE_SCHEMA_V1: &str = "paper-raid.artifact-bundle.v1";
pub const ARTIFACT_MANIFEST_BINDING_SCHEMA_V1: &str =
    "hepta.paper_raid.artifact_manifest_binding.v1";
pub const ARTIFACT_BUNDLE_ADAPTER_CONTRACT_HASH_V1: &str =
    "sha256:aba8fd6d1059c59f63cdb258a2e507de1bed3ff74f935b7dd0214e4640ad9bb6";
pub const ARTIFACT_BUNDLE_ADAPTER_SOURCE_REVISION: &str =
    "61c9ffd0b410faed023b68a604e4d0906c3006f8";

#[derive(Clone, Default)]
pub(super) struct CollaborationMemory {
    tickets: HashMap<Uuid, MatchmakingTicket>,
    team_proposals: HashMap<Uuid, TeamProposal>,
    team_proposal_decisions: HashMap<Uuid, TeamProposalDecision>,
    pub(super) artifact_manifests: HashMap<Uuid, ArtifactManifest>,
    revision_artifact_bindings: HashMap<Uuid, PaperRevisionArtifactBinding>,
    evidence_cards: HashMap<Uuid, EvidenceCard>,
    citations: HashMap<Uuid, CitationRecord>,
    experiment_plans: HashMap<Uuid, ExperimentPlan>,
    runs: HashMap<Uuid, RunRecord>,
    figures: HashMap<Uuid, FigureLineage>,
    claims: HashMap<Uuid, ClaimRecord>,
    section_heads: HashMap<(Uuid, String), SectionHead>,
    leases: HashMap<Uuid, SectionLease>,
    pub(super) proposals: HashMap<Uuid, AgentProposal>,
    pub(super) decisions: HashMap<Uuid, HumanDecision>,
    section_revisions: HashMap<Uuid, SectionRevision>,
    pub(super) section_reviews: HashMap<Uuid, SectionReview>,
    section_merges: HashMap<Uuid, SectionMerge>,
    events: Vec<PaperRoomEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchmakingTicketStatus {
    Queued,
    Matched,
    Cancelled,
    Expired,
}

impl MatchmakingTicketStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Matched => "matched",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchmakingTicket {
    pub ticket_id: Uuid,
    pub player_id: Uuid,
    pub challenge_id: Uuid,
    pub requested_team_size: u32,
    pub roles: Vec<String>,
    pub availability_hash: String,
    pub status: MatchmakingTicketStatus,
    pub matched_proposal_id: Option<Uuid>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMatchmakingTicketRequest {
    pub ticket_id: Uuid,
    pub challenge_id: Uuid,
    pub requested_team_size: u32,
    pub roles: Vec<String>,
    pub availability_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamProposalStatus {
    Proposed,
    Accepted,
    Declined,
    Expired,
}

impl TeamProposalStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamProposal {
    pub proposal_id: Uuid,
    pub challenge_id: Uuid,
    pub requested_team_size: u32,
    pub deterministic_match_key: String,
    pub member_player_ids: Vec<Uuid>,
    pub source_ticket_ids: Vec<Uuid>,
    pub status: TeamProposalStatus,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchmakingTicketResponse {
    pub ticket: MatchmakingTicket,
    pub team_proposal: Option<TeamProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamProposalDecisionKind {
    Accept,
    Decline,
}

impl TeamProposalDecisionKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamProposalDecision {
    pub decision_id: Uuid,
    pub proposal_id: Uuid,
    pub player_id: Uuid,
    pub decision: TeamProposalDecisionKind,
    pub proposal_version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTeamProposalDecisionRequest {
    pub decision_id: Uuid,
    pub expected_proposal_version: u64,
    pub decision: TeamProposalDecisionKind,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamProposalDecisionResponse {
    pub decision: TeamProposalDecision,
    pub proposal: TeamProposal,
    pub replacement_proposal: Option<TeamProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterializeTeamProposalRequest {
    pub expected_proposal_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperRaidProgress {
    pub paper_project_id: Uuid,
    pub title: String,
    pub phase: PaperPhase,
    pub version: u64,
    pub current_revision_id: Option<Uuid>,
    pub release_candidate_revision_id: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerRaidSummary {
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub team_status: TeamStatus,
    pub team_version: u64,
    pub roster_version: u64,
    pub member_count: usize,
    pub acceptance_count: usize,
    pub participant_slot: u32,
    pub role: String,
    pub player_ready: bool,
    pub paper: Option<PaperRaidProgress>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayerRaidState {
    pub schema: String,
    pub player_id: Uuid,
    pub current_raid: Option<PlayerRaidSummary>,
    pub raids: Vec<PlayerRaidSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicChallenge {
    pub challenge_id: Uuid,
    pub title: String,
    pub description: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub dataset_manifest_hash: String,
    pub status: crate::ChallengeStatus,
    pub created_at: DateTime<Utc>,
}

impl From<crate::ResearchChallenge> for PublicChallenge {
    fn from(challenge: crate::ResearchChallenge) -> Self {
        Self {
            challenge_id: challenge.challenge_id,
            title: challenge.title,
            description: challenge.description,
            ruleset_version: challenge.ruleset_version,
            ruleset_hash: challenge.ruleset_hash,
            dataset_manifest_hash: challenge.dataset_manifest_hash,
            status: challenge.status,
            created_at: challenge.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactAcl {
    Team,
    Reviewers,
    PublicAfterRelease,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub logical_path: String,
    pub sha256: String,
    pub uri: String,
    pub acl: ArtifactAcl,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NeutralArtifactRootV1 {
    pub algorithm: String,
    pub digest_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NeutralArtifactObjectV1 {
    pub canonical_json: bool,
    pub dependencies: Vec<String>,
    pub logical_path: String,
    pub media_type: String,
    pub role: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NeutralArtifactBundleV1 {
    pub artifact_root: NeutralArtifactRootV1,
    pub bundle_id: String,
    pub challenge_id: String,
    pub created_at: String,
    pub hepta_binding_status: String,
    pub human_authority_materialized: bool,
    pub object_count: u64,
    pub objects: Vec<NeutralArtifactObjectV1>,
    pub required_run_ids: Vec<String>,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactManifest {
    pub manifest_id: Uuid,
    pub paper_project_id: Uuid,
    pub binding_schema: String,
    pub source_bundle_schema: String,
    pub source_bundle_id: String,
    pub source_challenge_id: String,
    pub source_created_at: String,
    pub source_manifest_sha256: String,
    pub manifest_hash: String,
    pub object_count: u64,
    pub objects: Vec<NeutralArtifactObjectV1>,
    pub required_run_ids: Vec<String>,
    pub storage_locations: Vec<ArtifactReference>,
    pub total_size_bytes: u64,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperRevisionArtifactBinding {
    pub revision_id: Uuid,
    pub paper_project_id: Uuid,
    pub manifest_id: Uuid,
    pub artifact_manifest_hash: String,
    pub source_logical_path: String,
    pub source_manifest_hash: String,
    pub bibliography_logical_path: String,
    pub bibliography_hash: String,
    pub claim_evidence_graph_logical_path: String,
    pub claim_evidence_graph_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateArtifactManifestRequest {
    pub manifest_id: Uuid,
    pub expected_paper_version: u64,
    pub expected_source_manifest_sha256: String,
    pub source_bundle: NeutralArtifactBundleV1,
    pub storage_locations: Vec<ArtifactReference>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceCard {
    pub evidence_card_id: Uuid,
    pub paper_project_id: Uuid,
    pub source_uri: String,
    pub source_hash: String,
    pub locator: String,
    pub license: String,
    pub verified_by_player_id: Uuid,
    pub verification_key_id: String,
    pub verification_public_key: String,
    pub verification_public_key_hash: String,
    pub signed_at_unix: i64,
    pub verification_signature: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateEvidenceCardRequest {
    pub evidence_card_id: Uuid,
    pub source_uri: String,
    pub source_hash: String,
    pub locator: String,
    pub license: String,
    pub verification_key_id: String,
    pub verification_public_key: String,
    pub verification_public_key_hash: String,
    pub signed_at_unix: i64,
    pub verification_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CitationRecord {
    pub citation_id: Uuid,
    pub paper_project_id: Uuid,
    pub evidence_card_id: Uuid,
    pub doi: Option<String>,
    pub canonical_url: Option<String>,
    pub source_hash: String,
    pub locator: String,
    pub license: String,
    pub verified_by_player_id: Uuid,
    pub verification_key_id: String,
    pub verification_public_key: String,
    pub verification_public_key_hash: String,
    pub signed_at_unix: i64,
    pub verification_signature: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCitationRecordRequest {
    pub citation_id: Uuid,
    pub evidence_card_id: Uuid,
    pub doi: Option<String>,
    pub canonical_url: Option<String>,
    pub source_hash: String,
    pub locator: String,
    pub license: String,
    pub verification_key_id: String,
    pub verification_public_key: String,
    pub verification_public_key_hash: String,
    pub signed_at_unix: i64,
    pub verification_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExperimentPlan {
    pub experiment_plan_id: Uuid,
    pub paper_project_id: Uuid,
    pub protocol_snapshot_hash: String,
    pub code_manifest_id: Uuid,
    pub dataset_manifest_id: Uuid,
    pub environment_manifest_id: Uuid,
    pub seed_policy_hash: String,
    pub stopping_rule_hash: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateExperimentPlanRequest {
    pub experiment_plan_id: Uuid,
    pub protocol_snapshot_hash: String,
    pub code_manifest_id: Uuid,
    pub dataset_manifest_id: Uuid,
    pub environment_manifest_id: Uuid,
    pub seed_policy_hash: String,
    pub stopping_rule_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl RunStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    pub run_record_id: Uuid,
    pub paper_project_id: Uuid,
    pub experiment_plan_id: Uuid,
    pub status: RunStatus,
    pub seed: i64,
    pub parameters_hash: String,
    pub logs_manifest_id: Uuid,
    pub outputs_manifest_id: Option<Uuid>,
    pub metrics_hash: Option<String>,
    pub failure_hash: Option<String>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRunRecordRequest {
    pub run_record_id: Uuid,
    pub experiment_plan_id: Uuid,
    pub status: RunStatus,
    pub seed: i64,
    pub parameters_hash: String,
    pub logs_manifest_id: Uuid,
    pub outputs_manifest_id: Option<Uuid>,
    pub metrics_hash: Option<String>,
    pub failure_hash: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FigureLineage {
    pub figure_lineage_id: Uuid,
    pub paper_project_id: Uuid,
    pub figure_key: String,
    pub figure_manifest_id: Uuid,
    pub run_record_ids: Vec<Uuid>,
    pub transform_hash: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFigureLineageRequest {
    pub figure_lineage_id: Uuid,
    pub figure_key: String,
    pub figure_manifest_id: Uuid,
    pub run_record_ids: Vec<Uuid>,
    pub transform_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    Main,
    Numeric,
    Figure,
    Supporting,
    Limitation,
}

impl ClaimKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Numeric => "numeric",
            Self::Figure => "figure",
            Self::Supporting => "supporting",
            Self::Limitation => "limitation",
        }
    }

    fn requires_lineage(&self) -> bool {
        matches!(self, Self::Main | Self::Numeric | Self::Figure)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimRecord {
    pub claim_id: Uuid,
    pub paper_project_id: Uuid,
    pub claim_key: String,
    pub claim_kind: ClaimKind,
    pub statement_hash: String,
    pub evidence_card_ids: Vec<Uuid>,
    pub run_record_ids: Vec<Uuid>,
    pub figure_lineage_ids: Vec<Uuid>,
    pub status: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateClaimRecordRequest {
    pub claim_id: Uuid,
    pub claim_key: String,
    pub claim_kind: ClaimKind,
    pub statement_hash: String,
    pub evidence_card_ids: Vec<Uuid>,
    pub run_record_ids: Vec<Uuid>,
    pub figure_lineage_ids: Vec<Uuid>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionLeaseStatus {
    Active,
    Released,
    Expired,
    Consumed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionLease {
    pub lease_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_key: String,
    pub holder_player_id: Uuid,
    pub holder_binding_id: Uuid,
    pub fencing_token: u64,
    pub status: SectionLeaseStatus,
    pub version: u64,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionHead {
    pub paper_project_id: Uuid,
    pub section_key: String,
    pub base_paper_revision_id: Uuid,
    pub current_head_revision_id: Uuid,
    pub fencing_token: u64,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquireSectionLeaseRequest {
    pub lease_id: Uuid,
    pub section_key: String,
    pub holder_binding_id: Uuid,
    pub expected_previous_fencing_token: u64,
    pub ttl_seconds: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalKind {
    Proposal,
    Delivery,
}

impl AgentProposalKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Proposal => "proposal",
            Self::Delivery => "delivery",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalStatus {
    Submitted,
    Accepted,
    Rework,
    Rejected,
    Superseded,
}

impl AgentProposalStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
            Self::Rework => "rework",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentProposal {
    pub proposal_id: Uuid,
    pub paper_project_id: Uuid,
    pub work_item_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub proposal_kind: AgentProposalKind,
    pub payload_hash: String,
    pub artifact_manifest_id: Uuid,
    pub artifact_manifest_hash: String,
    pub agent_id: String,
    pub binding_id: Uuid,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub status: AgentProposalStatus,
    pub version: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentProposalRequest {
    pub proposal_id: Uuid,
    pub work_item_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub proposal_kind: AgentProposalKind,
    pub payload_hash: String,
    pub artifact_manifest_id: Uuid,
    pub agent_id: String,
    pub binding_id: Uuid,
    pub agent_key_id: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HumanDecisionKind {
    Accept,
    Rework,
    Reject,
}

impl HumanDecisionKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Rework => "rework",
            Self::Reject => "reject",
        }
    }

    fn proposal_status(&self) -> AgentProposalStatus {
        match self {
            Self::Accept => AgentProposalStatus::Accepted,
            Self::Rework => AgentProposalStatus::Rework,
            Self::Reject => AgentProposalStatus::Rejected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanDecision {
    pub decision_id: Uuid,
    pub paper_project_id: Uuid,
    pub proposal_id: Uuid,
    pub player_id: Uuid,
    pub decision: HumanDecisionKind,
    pub reason_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHumanDecisionRequest {
    pub decision_id: Uuid,
    pub proposal_id: Uuid,
    pub expected_proposal_version: u64,
    pub decision: HumanDecisionKind,
    pub reason_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionRevisionStatus {
    Proposed,
    Approved,
    Rework,
    Rejected,
    Merged,
    Superseded,
}

impl SectionRevisionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Rework => "rework",
            Self::Rejected => "rejected",
            Self::Merged => "merged",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionRevision {
    pub section_revision_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub proposal_id: Uuid,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub patch_manifest_id: Uuid,
    pub patch_hash: String,
    pub status: SectionRevisionStatus,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSectionRevisionRequest {
    pub section_revision_id: Uuid,
    pub section_key: String,
    pub parent_revision_id: Uuid,
    pub proposal_id: Uuid,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub patch_manifest_id: Uuid,
    pub patch_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionReviewVerdict {
    Approve,
    Rework,
    Reject,
}

impl SectionReviewVerdict {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Rework => "rework",
            Self::Reject => "reject",
        }
    }

    fn revision_status(&self) -> SectionRevisionStatus {
        match self {
            Self::Approve => SectionRevisionStatus::Approved,
            Self::Rework => SectionRevisionStatus::Rework,
            Self::Reject => SectionRevisionStatus::Rejected,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionReview {
    pub review_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_revision_id: Uuid,
    pub reviewer_player_id: Uuid,
    pub verdict: SectionReviewVerdict,
    pub review_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSectionReviewRequest {
    pub review_id: Uuid,
    pub expected_revision_version: u64,
    pub verdict: SectionReviewVerdict,
    pub review_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionMerge {
    pub merge_id: Uuid,
    pub paper_project_id: Uuid,
    pub section_key: String,
    pub section_revision_id: Uuid,
    pub parent_revision_id: Uuid,
    pub merged_section_revision_id: Uuid,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub merged_by_player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub merged_at_unix: i64,
    pub signature: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSectionMergeRequest {
    pub merge_id: Uuid,
    pub section_revision_id: Uuid,
    pub expected_revision_version: u64,
    pub parent_revision_id: Uuid,
    pub merged_section_revision_id: Uuid,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub merged_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaperRoomEvent {
    pub cursor: u64,
    pub event_id: Uuid,
    pub paper_project_id: Uuid,
    pub event_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaperRoomReadModel {
    pub paper: PaperProject,
    pub team: ResearchTeam,
    pub team_member_acceptances: Vec<TeamMemberAcceptance>,
    pub work_items: Vec<WorkItem>,
    pub paper_revisions: Vec<PaperRevision>,
    pub authorship_consents: Vec<AuthorshipConsent>,
    pub joint_submission: Option<JointPaperSubmission>,
    pub member_research_sessions: Vec<MemberResearchSessionAccess>,
    pub artifact_manifests: Vec<ArtifactManifest>,
    pub revision_artifact_bindings: Vec<PaperRevisionArtifactBinding>,
    pub evidence_cards: Vec<EvidenceCard>,
    pub citations: Vec<CitationRecord>,
    pub experiment_plans: Vec<ExperimentPlan>,
    pub runs: Vec<RunRecord>,
    pub figures: Vec<FigureLineage>,
    pub claims: Vec<ClaimRecord>,
    pub section_heads: Vec<SectionHead>,
    pub leases: Vec<SectionLease>,
    pub proposals: Vec<AgentProposal>,
    pub decisions: Vec<HumanDecision>,
    pub section_revisions: Vec<SectionRevision>,
    pub section_reviews: Vec<SectionReview>,
    pub section_merges: Vec<SectionMerge>,
    pub last_event_cursor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemberResearchSessionAccess {
    pub logical_session_id: String,
    pub authorization_set_id: Uuid,
    pub roster_version: u64,
    pub authorization_id: Uuid,
    pub status: ResearchSessionAuthorizationSetStatus,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub nakama_completion_received: bool,
}

#[derive(Debug, Deserialize)]
pub struct EventCursorQuery {
    #[serde(default)]
    pub after_cursor: u64,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/v2/hepta/challenges", get(list_public_challenges))
        .route(
            "/v2/hepta/challenges/:challenge_id",
            get(get_public_challenge),
        )
        .route(
            "/v2/hepta/matchmaking/tickets",
            get(list_matchmaking_tickets).post(create_matchmaking_ticket),
        )
        .route(
            "/v2/hepta/matchmaking/tickets/:ticket_id",
            get(get_matchmaking_ticket),
        )
        .route("/v2/hepta/team-proposals", get(list_team_proposals))
        .route(
            "/v2/hepta/team-proposals/:proposal_id",
            get(get_team_proposal),
        )
        .route(
            "/v2/hepta/team-proposals/:proposal_id/materialize",
            post(materialize_team_proposal),
        )
        .route(
            "/v2/hepta/team-proposals/:proposal_id/decisions",
            post(create_team_proposal_decision),
        )
        .route("/v2/hepta/raid-state", get(get_player_raid_state))
        .route(
            "/v2/hepta/papers/:paper_id/artifact-manifests",
            post(create_artifact_manifest),
        )
        .route(
            "/v2/hepta/papers/:paper_id/evidence-cards",
            post(create_evidence_card),
        )
        .route(
            "/v2/hepta/papers/:paper_id/citations",
            post(create_citation_record),
        )
        .route(
            "/v2/hepta/papers/:paper_id/experiment-plans",
            post(create_experiment_plan),
        )
        .route(
            "/v2/hepta/papers/:paper_id/run-records",
            post(create_run_record),
        )
        .route(
            "/v2/hepta/papers/:paper_id/figure-lineage",
            post(create_figure_lineage),
        )
        .route(
            "/v2/hepta/papers/:paper_id/claims",
            post(create_claim_record),
        )
        .route(
            "/v2/hepta/papers/:paper_id/section-leases",
            post(acquire_section_lease),
        )
        .route(
            "/v2/hepta/papers/:paper_id/agent-proposals",
            post(create_agent_proposal),
        )
        .route(
            "/v2/hepta/papers/:paper_id/human-decisions",
            post(create_human_decision),
        )
        .route(
            "/v2/hepta/papers/:paper_id/section-revisions",
            post(create_section_revision),
        )
        .route(
            "/v2/hepta/papers/:paper_id/section-revisions/:section_revision_id/reviews",
            post(create_section_review),
        )
        .route(
            "/v2/hepta/papers/:paper_id/section-merges",
            post(create_section_merge),
        )
        .route("/v2/hepta/papers/:paper_id/room", get(get_paper_room))
        .route(
            "/v2/hepta/papers/:paper_id/events",
            get(list_paper_room_events),
        )
}

#[derive(Debug, Clone, Copy)]
enum CollaborationMutation {
    ArtifactManifest,
    Evidence,
    Citation,
    ExperimentPlan,
    Run,
    Figure,
    Claim,
    SectionDraft,
    SectionReview,
}

impl CollaborationMutation {
    fn label(self) -> &'static str {
        match self {
            Self::ArtifactManifest => "artifact_manifest",
            Self::Evidence => "evidence",
            Self::Citation => "citation",
            Self::ExperimentPlan => "experiment_plan",
            Self::Run => "run",
            Self::Figure => "figure",
            Self::Claim => "claim",
            Self::SectionDraft => "section_draft",
            Self::SectionReview => "section_review",
        }
    }

    fn allowed(self, phase: PaperPhase) -> bool {
        match self {
            Self::ArtifactManifest => matches!(
                phase,
                PaperPhase::Preregistering
                    | PaperPhase::Researching
                    | PaperPhase::Experimenting
                    | PaperPhase::Drafting
                    | PaperPhase::IntegrityReview
                    | PaperPhase::Reproducing
            ),
            Self::Evidence | Self::Citation | Self::Claim => matches!(
                phase,
                PaperPhase::Researching
                    | PaperPhase::Experimenting
                    | PaperPhase::Drafting
                    | PaperPhase::IntegrityReview
                    | PaperPhase::Reproducing
            ),
            Self::ExperimentPlan => {
                matches!(phase, PaperPhase::Preregistering | PaperPhase::Researching)
            }
            Self::Run | Self::Figure => matches!(
                phase,
                PaperPhase::Experimenting
                    | PaperPhase::Drafting
                    | PaperPhase::IntegrityReview
                    | PaperPhase::Reproducing
            ),
            Self::SectionDraft => matches!(phase, PaperPhase::Drafting),
            Self::SectionReview => {
                matches!(phase, PaperPhase::Drafting | PaperPhase::IntegrityReview)
            }
        }
    }
}

fn require_collaboration_phase(
    paper: &PaperProject,
    mutation: CollaborationMutation,
) -> Result<(), ApiError> {
    if mutation.allowed(paper.phase) {
        return Ok(());
    }
    Err(ApiError::conflict(
        "paper_phase_disallows_collaboration_mutation",
        format!(
            "{} is not allowed while paper phase is {}",
            mutation.label(),
            paper.phase.as_str()
        ),
    ))
}

fn validate_collaboration_text(field: &'static str, value: &str) -> Result<(), ApiError> {
    validate_contract_text_api(field, value)?;
    if value.len() > 512 {
        return Err(ApiError::bad_request(
            "invalid_collaboration_text",
            format!("{field} exceeds 512 bytes"),
        ));
    }
    Ok(())
}

fn validate_section_key(value: &str) -> Result<(), ApiError> {
    validate_collaboration_text("section_key", value)?;
    let mut bytes = value.bytes();
    let first = bytes
        .next()
        .ok_or_else(|| ApiError::bad_request("invalid_section_key", "section_key is required"))?;
    if value.len() > 128
        || !first.is_ascii_alphanumeric()
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ApiError::bad_request(
            "invalid_section_key",
            "section_key must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}",
        ));
    }
    Ok(())
}

fn validate_logical_path(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 240
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.contains('?')
        || value.contains('#')
        || value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err(ApiError::bad_request(
            "unsafe_artifact_path",
            "logical_path must be a normalized relative path without traversal",
        ));
    }
    Ok(())
}

fn validate_https_uri(value: &str) -> Result<(), ApiError> {
    let rest = value
        .strip_prefix("https://")
        .ok_or_else(|| ApiError::bad_request("unsafe_artifact_uri", "URI must use https"))?;
    let (authority, path) = rest.split_once('/').ok_or_else(|| {
        ApiError::bad_request("unsafe_artifact_uri", "https URI requires host and path")
    })?;
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains(':')
            && !authority.rsplit_once(':').is_some_and(|(_, port)| {
                !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
            })
        || path.is_empty()
        || path.contains('\\')
        || path.contains('?')
        || path.contains('#')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(ApiError::bad_request(
            "unsafe_artifact_uri",
            "https URI may not contain credentials, traversal, query, fragment, or controls",
        ));
    }
    Ok(())
}

fn validate_artifact_uri(uri: &str, digest: &str) -> Result<(), ApiError> {
    if let Some(hex) = uri.strip_prefix("cas://sha256/") {
        if digest != format!("sha256:{hex}") {
            return Err(ApiError::bad_request(
                "artifact_uri_digest_mismatch",
                "CAS URI must name the exact artifact digest",
            ));
        }
        validate_digest_v2("artifact_uri", digest)?;
        return Ok(());
    }
    if let Some(git_uri) = uri.strip_prefix("git+") {
        let (repository, revision_path) = git_uri.rsplit_once('@').ok_or_else(|| {
            ApiError::bad_request(
                "unsafe_artifact_uri",
                "Git URI must pin one immutable commit",
            )
        })?;
        validate_https_uri(repository)?;
        let (revision, path) = revision_path.split_once('/').ok_or_else(|| {
            ApiError::bad_request(
                "unsafe_artifact_uri",
                "Git URI must include a commit and normalized path",
            )
        })?;
        if revision.len() != 40
            || revision
                .bytes()
                .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
        {
            return Err(ApiError::bad_request(
                "unsafe_artifact_uri",
                "Git URI commit must be 40 lowercase hexadecimal characters",
            ));
        }
        validate_logical_path(path)?;
        return Ok(());
    }
    Err(ApiError::bad_request(
        "unsafe_artifact_uri",
        "artifact URI must use cas://sha256/... or git+https://...@<commit>/<path>",
    ))
}

fn validate_public_source_uri(value: &str) -> Result<(), ApiError> {
    if value.len() > 480 {
        return Err(ApiError::bad_request(
            "unsafe_source_uri",
            "source URI exceeds 480 bytes",
        ));
    }
    validate_https_uri(value).map_err(|_| {
        ApiError::bad_request(
            "unsafe_source_uri",
            "source URI must be a credential-free canonical https URL",
        )
    })
}

fn evidence_source_identifier(uri: &str) -> String {
    format!("evidence-uri\n{uri}")
}

fn citation_source_identifier(doi: Option<&str>, canonical_url: Option<&str>) -> String {
    format!(
        "citation-doi\n{}\ncitation-url\n{}",
        doi.unwrap_or("-"),
        canonical_url.unwrap_or("-")
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

fn deterministic_uuid(label: &str) -> Uuid {
    let hash = Sha256::digest(label.as_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
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

async fn require_registered_player_read(
    state: &AppState,
    headers: &HeaderMap,
    operation: &str,
    canonical_path: &str,
) -> Result<ConsumerUserAssertionClaimV2, ApiError> {
    let assertion = require_member_read_assertion(headers, state, operation, canonical_path)?;
    if let Some(pool) = &state.pool {
        let row = sqlx::query("select record_json from hepta_human_players where player_id = $1")
            .bind(assertion.player_id)
            .fetch_optional(pool)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::forbidden(
                    "human_player_not_registered",
                    "read assertion does not identify a registered human player",
                )
            })?;
        let player: HumanPlayer = decode_record(row.get("record_json"), "human player")?;
        assert_player_identity(&assertion, &player)?;
    } else {
        let memory = state.paper_raid.read().await;
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden(
                "human_player_not_registered",
                "read assertion does not identify a registered human player",
            )
        })?;
        assert_player_identity(&assertion, player)?;
    }
    Ok(assertion)
}

fn paper_and_team_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<(PaperProject, ResearchTeam), ApiError> {
    let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let team = memory
        .teams
        .get(&paper.team_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("paper project research team record is missing"))?;
    assert_team_actor_memory(memory, &team, assertion)?;
    Ok((paper, team))
}

async fn paper_and_team_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<(PaperProject, ResearchTeam), ApiError> {
    let paper_row = sqlx::query(
        "select record_json from hepta_paper_projects where paper_project_id = $1 for share",
    )
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    assert_team_actor_postgres(tx, paper.team_id, assertion).await?;
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id = $1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::database)?;
    let team = decode_record(team_row.get("record_json"), "research team")?;
    Ok((paper, team))
}

fn active_human_key_memory(
    memory: &PaperRaidMemory,
    player_id: Uuid,
    key_id: &str,
    public_key: &str,
    public_key_hash: &str,
    signed_at_unix: i64,
) -> Result<ed25519_dalek::VerifyingKey, ApiError> {
    let player = memory.players.get(&player_id).ok_or_else(|| {
        ApiError::forbidden("human_player_not_registered", "human player does not exist")
    })?;
    let key = memory
        .human_signing_keys
        .get(&(player_id, key_id.to_string()))
        .ok_or_else(|| ApiError::forbidden("human_key_not_found", "human signing key not found"))?;
    if key.status != HumanSigningKeyStatus::Active
        || player.signing_key_id != key_id
        || key.signing_public_key != public_key
        || key.signing_public_key_hash != public_key_hash
        || key.registered_at.timestamp() > signed_at_unix
    {
        return Err(ApiError::forbidden(
            "human_key_not_current",
            "human decision must use the current active signing key",
        ));
    }
    canonical_public_key(public_key).and_then(|canonical| crate::decode_verifying_key(&canonical))
}

async fn active_human_key_postgres(
    tx: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
    key_id: &str,
    public_key: &str,
    public_key_hash: &str,
    signed_at_unix: i64,
) -> Result<ed25519_dalek::VerifyingKey, ApiError> {
    let row = sqlx::query(
        "select p.signing_key_id as current_key_id, k.signing_public_key,
                k.signing_public_key_hash, k.status, k.registered_at
         from hepta_human_players p
         join hepta_human_signing_keys k on k.player_id = p.player_id
         where p.player_id = $1 and k.signing_key_id = $2
         for share of p, k",
    )
    .bind(player_id)
    .bind(key_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::forbidden("human_key_not_found", "human signing key not found"))?;
    let stored_public_key: String = row.get("signing_public_key");
    let stored_hash: String = row.get("signing_public_key_hash");
    let registered_at: DateTime<Utc> = row.get("registered_at");
    if row.get::<String, _>("current_key_id") != key_id
        || row.get::<String, _>("status") != "active"
        || stored_public_key != public_key
        || stored_hash != public_key_hash
        || registered_at.timestamp() > signed_at_unix
    {
        return Err(ApiError::forbidden(
            "human_key_not_current",
            "human decision must use the current active signing key",
        ));
    }
    canonical_public_key(public_key).and_then(|canonical| crate::decode_verifying_key(&canonical))
}

// The explicit event envelope fields mirror the durable outbox boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_room_event_memory(
    memory: &mut PaperRaidMemory,
    operation: &str,
    idempotency_key: &str,
    event_type: &str,
    paper_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
) {
    let event = PaperRoomEvent {
        cursor: u64::try_from(memory.collaboration.events.len() + 1)
            .expect("memory event count fits u64"),
        event_id: Uuid::new_v4(),
        paper_project_id: paper_id,
        event_type: event_type.to_string(),
        aggregate_id,
        aggregate_version,
        payload: payload.clone(),
        occurred_at: Utc::now(),
    };
    memory.collaboration.events.push(event);
    push_memory_event(
        memory,
        operation,
        idempotency_key,
        event_type,
        aggregate_id,
        aggregate_version,
        payload,
    )
    .expect("Paper Room event payload is valid JSON");
}

// Keep the PostgreSQL event call shape byte-for-byte parallel with memory.
#[allow(clippy::too_many_arguments)]
pub(super) async fn insert_room_event_postgres(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    idempotency_key: &str,
    event_type: &str,
    paper_id: Uuid,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
) -> Result<(), ApiError> {
    let event_id = Uuid::new_v4();
    let occurred_at = Utc::now();
    sqlx::query(
        "insert into hepta_paper_room_events (
            event_id, paper_project_id, event_type, aggregate_id,
            aggregate_version, payload, occurred_at
         ) values ($1,$2,$3,$4,$5,$6::jsonb,$7)",
    )
    .bind(event_id)
    .bind(paper_id)
    .bind(event_type)
    .bind(aggregate_id)
    .bind(i64::try_from(aggregate_version).map_err(|_| {
        ApiError::bad_request(
            "invalid_version",
            "aggregate version exceeds PostgreSQL bigint",
        )
    })?)
    .bind(payload.clone())
    .bind(occurred_at)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    insert_postgres_event(
        tx,
        operation,
        idempotency_key,
        event_type,
        aggregate_id,
        aggregate_version,
        payload,
    )
    .await
}

async fn list_public_challenges(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PublicChallenge>>, ApiError> {
    require_registered_player_read(
        &state,
        &headers,
        "list_public_challenges_v2",
        "/v2/hepta/challenges",
    )
    .await?;
    state
        .inspect(|league| {
            let mut challenges: Vec<_> = league
                .challenges
                .values()
                .filter(|challenge| challenge.status != crate::ChallengeStatus::Draft)
                .cloned()
                .map(PublicChallenge::from)
                .collect();
            challenges.sort_by_key(|challenge| (challenge.created_at, challenge.challenge_id));
            Ok(Json(challenges))
        })
        .await
}

async fn get_public_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(challenge_id): Path<Uuid>,
) -> Result<Json<PublicChallenge>, ApiError> {
    let path = format!("/v2/hepta/challenges/{challenge_id}");
    require_registered_player_read(&state, &headers, "get_public_challenge_v2", &path).await?;
    state
        .inspect(|league| {
            league
                .challenges
                .get(&challenge_id)
                .filter(|challenge| challenge.status != crate::ChallengeStatus::Draft)
                .cloned()
                .map(PublicChallenge::from)
                .map(Json)
                .ok_or_else(|| {
                    ApiError::not_found(
                        "challenge_not_found",
                        "public research challenge does not exist",
                    )
                })
        })
        .await
}

fn validate_matchmaking_ticket_request(
    request: &CreateMatchmakingTicketRequest,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("availability_hash", &request.availability_hash)?;
    if request.requested_team_size != 3 {
        return Err(ApiError::bad_request(
            "alpha_team_size_requires_three",
            "the v0 alpha matcher supports exactly three players; Team and protocol aggregates remain 3-5",
        ));
    }
    if request.roles.is_empty() || request.roles.len() > 5 {
        return Err(ApiError::bad_request(
            "invalid_matchmaking_roles",
            "matchmaking ticket requires 1-5 distinct roles",
        ));
    }
    let mut roles = HashSet::new();
    for role in &request.roles {
        validate_collaboration_text("role", role)?;
        if !roles.insert(role) {
            return Err(ApiError::bad_request(
                "duplicate_matchmaking_role",
                "matchmaking roles must be unique",
            ));
        }
    }
    Ok(())
}

fn select_alpha_match(tickets: impl Iterator<Item = MatchmakingTicket>) -> Vec<MatchmakingTicket> {
    let mut tickets: Vec<_> = tickets
        .filter(|ticket| {
            ticket.status == MatchmakingTicketStatus::Queued && ticket.requested_team_size == 3
        })
        .collect();
    tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id));
    tickets.truncate(3);
    tickets
}

fn build_team_proposal(tickets: &[MatchmakingTicket]) -> Result<TeamProposal, ApiError> {
    if tickets.len() != 3
        || tickets
            .iter()
            .any(|ticket| ticket.challenge_id != tickets[0].challenge_id)
    {
        return Err(ApiError::internal(
            "deterministic alpha matcher received an invalid queue slice",
        ));
    }
    let source_ticket_ids: Vec<_> = tickets.iter().map(|ticket| ticket.ticket_id).collect();
    let member_player_ids: Vec<_> = tickets.iter().map(|ticket| ticket.player_id).collect();
    if member_player_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .len()
        != 3
    {
        return Err(ApiError::conflict(
            "duplicate_matchmaking_player",
            "one human player cannot occupy multiple Paper Raid team slots",
        ));
    }
    let match_key = crate::paper_raid_contracts::sha256_digest(
        format!(
            "{}:{}",
            tickets[0].challenge_id,
            source_ticket_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join(":")
        )
        .as_bytes(),
    );
    let now = Utc::now();
    Ok(TeamProposal {
        proposal_id: deterministic_uuid(&match_key),
        challenge_id: tickets[0].challenge_id,
        requested_team_size: 3,
        deterministic_match_key: match_key,
        member_player_ids,
        source_ticket_ids,
        status: TeamProposalStatus::Proposed,
        version: 1,
        created_at: now,
        updated_at: now,
    })
}

async fn create_matchmaking_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMatchmakingTicketRequest>,
) -> Result<(StatusCode, Json<MatchmakingTicketResponse>), ApiError> {
    const OPERATION: &str = "create_matchmaking_ticket_v3";
    const PATH: &str = "/v2/hepta/matchmaking/tickets";
    validate_matchmaking_ticket_request(&request)?;
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        PATH,
        &request.idempotency_key,
        &request_hash,
    )?;
    state
        .inspect(|league| {
            let challenge = league
                .challenges
                .get(&request.challenge_id)
                .ok_or_else(|| {
                    ApiError::not_found("challenge_not_found", "challenge does not exist")
                })?;
            if challenge.status != crate::ChallengeStatus::Open {
                return Err(ApiError::conflict(
                    "challenge_not_open",
                    "matchmaking requires an open challenge",
                ));
            }
            Ok(())
        })
        .await?;

    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden("human_player_not_registered", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if memory.collaboration.tickets.values().any(|ticket| {
            ticket.player_id == assertion.player_id
                && ticket.challenge_id == request.challenge_id
                && ticket.status == MatchmakingTicketStatus::Queued
        }) {
            return Err(ApiError::conflict(
                "live_matchmaking_ticket_exists",
                "player already has a queued ticket for this challenge",
            ));
        }
        let now = Utc::now();
        let ticket = MatchmakingTicket {
            ticket_id: request.ticket_id,
            player_id: assertion.player_id,
            challenge_id: request.challenge_id,
            requested_team_size: request.requested_team_size,
            roles: request.roles.clone(),
            availability_hash: request.availability_hash.clone(),
            status: MatchmakingTicketStatus::Queued,
            matched_proposal_id: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        if memory
            .collaboration
            .tickets
            .insert(ticket.ticket_id, ticket.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "matchmaking_ticket_exists",
                "ticket_id already exists",
            ));
        }
        let selected = select_alpha_match(
            memory
                .collaboration
                .tickets
                .values()
                .filter(|candidate| candidate.challenge_id == request.challenge_id)
                .cloned(),
        );
        let proposal = if selected.len() == 3 {
            let proposal = build_team_proposal(&selected)?;
            for selected_ticket in &selected {
                let stored = memory
                    .collaboration
                    .tickets
                    .get_mut(&selected_ticket.ticket_id)
                    .expect("selected ticket exists");
                stored.status = MatchmakingTicketStatus::Matched;
                stored.matched_proposal_id = Some(proposal.proposal_id);
                stored.version += 1;
                stored.updated_at = now;
            }
            memory
                .collaboration
                .team_proposals
                .insert(proposal.proposal_id, proposal.clone());
            Some(proposal)
        } else {
            None
        };
        let response_ticket = memory
            .collaboration
            .tickets
            .get(&request.ticket_id)
            .expect("created ticket exists")
            .clone();
        let response = MatchmakingTicketResponse {
            ticket: response_ticket.clone(),
            team_proposal: proposal.clone(),
        };
        if let Some(proposal) = &proposal {
            push_memory_event(
                &mut memory,
                OPERATION,
                &format!("{}:team-proposal", request.idempotency_key),
                "hepta.paper_raid.team_proposal.created.v1",
                proposal.proposal_id,
                proposal.version,
                json!({
                    "proposal_id":proposal.proposal_id,
                    "challenge_id":proposal.challenge_id,
                    "member_player_ids":proposal.member_player_ids,
                }),
            )
            .expect("team proposal event payload is valid JSON");
        }
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.matchmaking_ticket.created.v1",
            response_ticket.ticket_id,
            response_ticket.version,
            json!({
                "ticket_id": response_ticket.ticket_id,
                "challenge_id": response_ticket.challenge_id,
                "status": response_ticket.status,
                "matched_proposal_id": response_ticket.matched_proposal_id,
            }),
        )
        .expect("matchmaking event payload is valid JSON");
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &response,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        let response = serde_json::from_value(replay.response)
            .map_err(|error| ApiError::internal(format!("decode matchmaking replay: {error}")))?;
        return Ok((replay.status, Json(response)));
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-paper-raid-matchmaking:{}",
            request.challenge_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id = $1 for share")
            .bind(assertion.player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::forbidden("human_player_not_registered", "human player does not exist")
            })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let now = Utc::now();
    let mut ticket = MatchmakingTicket {
        ticket_id: request.ticket_id,
        player_id: assertion.player_id,
        challenge_id: request.challenge_id,
        requested_team_size: request.requested_team_size,
        roles: request.roles.clone(),
        availability_hash: request.availability_hash.clone(),
        status: MatchmakingTicketStatus::Queued,
        matched_proposal_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(
        "insert into hepta_matchmaking_tickets (
            ticket_id, player_id, challenge_id, requested_team_size, roles,
            availability_hash, status, matched_proposal_id, version, record_json,
            created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,'queued',null,1,$7::jsonb,$8,$8)",
    )
    .bind(ticket.ticket_id)
    .bind(ticket.player_id)
    .bind(ticket.challenge_id)
    .bind(i32::try_from(ticket.requested_team_size).map_err(|_| {
        ApiError::bad_request("invalid_team_size", "team size exceeds PostgreSQL integer")
    })?)
    .bind(&ticket.roles)
    .bind(&ticket.availability_hash)
    .bind(
        serde_json::to_value(&ticket)
            .map_err(|error| ApiError::internal(format!("encode matchmaking ticket: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => ApiError::conflict(
            "live_matchmaking_ticket_exists",
            "player already has a queued ticket or ticket_id exists",
        ),
        _ => ApiError::database(error),
    })?;
    let rows = sqlx::query(
        "select record_json from hepta_matchmaking_tickets
         where challenge_id = $1 and requested_team_size = 3 and status = 'queued'
         order by created_at, ticket_id
         for update",
    )
    .bind(request.challenge_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let selected = select_alpha_match(
        rows.into_iter()
            .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter(),
    );
    let proposal = if selected.len() == 3 {
        let proposal = build_team_proposal(&selected)?;
        sqlx::query(
            "insert into hepta_team_proposals (
                proposal_id, challenge_id, requested_team_size, deterministic_match_key,
                status, member_player_ids, source_ticket_ids, version, record_json,
                created_at, updated_at
             ) values ($1,$2,3,$3,'proposed',$4,$5,1,$6::jsonb,$7,$7)",
        )
        .bind(proposal.proposal_id)
        .bind(proposal.challenge_id)
        .bind(&proposal.deterministic_match_key)
        .bind(&proposal.member_player_ids)
        .bind(&proposal.source_ticket_ids)
        .bind(
            serde_json::to_value(&proposal)
                .map_err(|error| ApiError::internal(format!("encode team proposal: {error}")))?,
        )
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        for mut selected_ticket in selected {
            selected_ticket.status = MatchmakingTicketStatus::Matched;
            selected_ticket.matched_proposal_id = Some(proposal.proposal_id);
            selected_ticket.version += 1;
            selected_ticket.updated_at = now;
            let updated =
                sqlx::query(
                    "update hepta_matchmaking_tickets
                 set status = 'matched', matched_proposal_id = $1,
                     version = version + 1, record_json = $2::jsonb, updated_at = $3
                 where ticket_id = $4 and status = 'queued' and version = 1",
                )
                .bind(proposal.proposal_id)
                .bind(serde_json::to_value(&selected_ticket).map_err(|error| {
                    ApiError::internal(format!("encode matched ticket: {error}"))
                })?)
                .bind(now)
                .bind(selected_ticket.ticket_id)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "matchmaking_queue_changed",
                    "matchmaking queue changed during deterministic selection",
                ));
            }
            if selected_ticket.ticket_id == ticket.ticket_id {
                ticket = selected_ticket;
            }
        }
        insert_postgres_event(
            &mut tx,
            OPERATION,
            &format!("{}:team-proposal", request.idempotency_key),
            "hepta.paper_raid.team_proposal.created.v1",
            proposal.proposal_id,
            proposal.version,
            json!({
                "proposal_id":proposal.proposal_id,
                "challenge_id":proposal.challenge_id,
                "member_player_ids":proposal.member_player_ids,
            }),
        )
        .await?;
        Some(proposal)
    } else {
        None
    };
    let response = MatchmakingTicketResponse {
        ticket: ticket.clone(),
        team_proposal: proposal,
    };
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.matchmaking_ticket.created.v1",
        ticket.ticket_id,
        ticket.version,
        json!({
            "ticket_id":ticket.ticket_id,
            "challenge_id":ticket.challenge_id,
            "status":ticket.status,
            "matched_proposal_id":ticket.matched_proposal_id,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(ticket.ticket_id),
        StatusCode::CREATED,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn list_matchmaking_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MatchmakingTicket>>, ApiError> {
    let assertion = require_registered_player_read(
        &state,
        &headers,
        "list_matchmaking_tickets_v3",
        "/v2/hepta/matchmaking/tickets",
    )
    .await?;
    let mut tickets = if let Some(pool) = &state.pool {
        sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where player_id = $1 order by created_at, ticket_id",
        )
        .bind(assertion.player_id)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
        .collect::<Result<Vec<_>, _>>()?
    } else {
        state
            .paper_raid
            .read()
            .await
            .collaboration
            .tickets
            .values()
            .filter(|ticket| ticket.player_id == assertion.player_id)
            .cloned()
            .collect()
    };
    tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id));
    Ok(Json(tickets))
}

async fn get_matchmaking_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<MatchmakingTicket>, ApiError> {
    let path = format!("/v2/hepta/matchmaking/tickets/{ticket_id}");
    let assertion =
        require_registered_player_read(&state, &headers, "get_matchmaking_ticket_v3", &path)
            .await?;
    let ticket = if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where ticket_id = $1 and player_id = $2",
        )
        .bind(ticket_id)
        .bind(assertion.player_id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "matchmaking_ticket_not_found",
                "visible matchmaking ticket does not exist",
            )
        })?;
        decode_record(row.get("record_json"), "matchmaking ticket")?
    } else {
        state
            .paper_raid
            .read()
            .await
            .collaboration
            .tickets
            .get(&ticket_id)
            .filter(|ticket| ticket.player_id == assertion.player_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "matchmaking_ticket_not_found",
                    "visible matchmaking ticket does not exist",
                )
            })?
    };
    Ok(Json(ticket))
}

async fn list_team_proposals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<TeamProposal>>, ApiError> {
    let assertion = require_registered_player_read(
        &state,
        &headers,
        "list_team_proposals_v3",
        "/v2/hepta/team-proposals",
    )
    .await?;
    let mut proposals = if let Some(pool) = &state.pool {
        sqlx::query(
            "select record_json from hepta_team_proposals
             where $1 = any(member_player_ids)
             order by created_at, proposal_id",
        )
        .bind(assertion.player_id)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "team proposal"))
        .collect::<Result<Vec<_>, _>>()?
    } else {
        state
            .paper_raid
            .read()
            .await
            .collaboration
            .team_proposals
            .values()
            .filter(|proposal| proposal.member_player_ids.contains(&assertion.player_id))
            .cloned()
            .collect()
    };
    proposals.sort_by_key(|proposal| (proposal.created_at, proposal.proposal_id));
    Ok(Json(proposals))
}

async fn get_team_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(proposal_id): Path<Uuid>,
) -> Result<Json<TeamProposal>, ApiError> {
    let path = format!("/v2/hepta/team-proposals/{proposal_id}");
    let assertion =
        require_registered_player_read(&state, &headers, "get_team_proposal_v3", &path).await?;
    let proposal = if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select record_json from hepta_team_proposals
             where proposal_id = $1 and $2 = any(member_player_ids)",
        )
        .bind(proposal_id)
        .bind(assertion.player_id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "team_proposal_not_found",
                "visible team proposal does not exist",
            )
        })?;
        decode_record(row.get("record_json"), "team proposal")?
    } else {
        state
            .paper_raid
            .read()
            .await
            .collaboration
            .team_proposals
            .get(&proposal_id)
            .filter(|proposal| proposal.member_player_ids.contains(&assertion.player_id))
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "team_proposal_not_found",
                    "visible team proposal does not exist",
                )
            })?
    };
    Ok(Json(proposal))
}

fn first_playable_team(
    proposal: &TeamProposal,
    tickets: &[MatchmakingTicket],
    bindings: &[AgentBinding],
    now: DateTime<Utc>,
) -> Result<ResearchTeam, ApiError> {
    if proposal.status != TeamProposalStatus::Accepted
        || proposal.requested_team_size != 3
        || proposal.member_player_ids.len() != 3
        || proposal.source_ticket_ids.len() != 3
        || tickets.len() != 3
        || bindings.len() != 3
    {
        return Err(ApiError::conflict(
            "team_proposal_not_materializable",
            "first-playable materialization requires one accepted three-player proposal with exact tickets and bindings",
        ));
    }
    let mut used_roles = HashSet::new();
    let mut members = Vec::with_capacity(3);
    for (index, player_id) in proposal.member_player_ids.iter().enumerate() {
        let ticket_id = proposal.source_ticket_ids[index];
        let ticket = tickets
            .iter()
            .find(|ticket| ticket.ticket_id == ticket_id)
            .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))?;
        if ticket.player_id != *player_id
            || ticket.challenge_id != proposal.challenge_id
            || ticket.matched_proposal_id != Some(proposal.proposal_id)
            || ticket.status != MatchmakingTicketStatus::Matched
        {
            return Err(ApiError::conflict(
                "team_proposal_ticket_mismatch",
                "team proposal no longer matches its exact player tickets",
            ));
        }
        let role = ticket
            .roles
            .iter()
            .find(|role| !used_roles.contains(role.as_str()))
            .cloned()
            .ok_or_else(|| {
                ApiError::conflict(
                    "team_roles_not_complementary",
                    "first-playable materialization requires three distinct compatible roles",
                )
            })?;
        used_roles.insert(role.clone());
        let binding = bindings
            .iter()
            .find(|binding| binding.player_id == *player_id)
            .ok_or_else(|| ApiError::internal("materialization Agent binding is missing"))?;
        if binding.status != AgentBindingStatus::Active {
            return Err(ApiError::conflict(
                "agent_binding_not_active",
                "materialization requires one active Agent binding per player",
            ));
        }
        members.push(TeamMember {
            participant_slot: u32::try_from(index + 1)
                .map_err(|_| ApiError::internal("participant slot exceeds u32"))?,
            player_id: *player_id,
            binding_id: binding.binding_id,
            agent_id: binding.agent_id.clone(),
            role,
            joined_at: now.to_owned(),
        });
    }
    let compact = json!({
        "schema": "hepta.paper_raid.first_playable_compact.v1",
        "proposal_id": proposal.proposal_id,
        "challenge_id": proposal.challenge_id,
        "deterministic_match_key": &proposal.deterministic_match_key,
        "members": members.iter().map(|member| json!({
            "participant_slot": member.participant_slot,
            "player_id": member.player_id,
            "binding_id": member.binding_id,
            "agent_id": &member.agent_id,
            "role": &member.role,
        })).collect::<Vec<_>>(),
        "research_authority": "hepta",
        "realtime_authority": "nakama",
        "settlement_eligibility": false,
    });
    let compact_bytes = canonical_json_bytes(&compact).map_err(|error| {
        ApiError::internal(format!("canonicalize collaboration compact: {error}"))
    })?;
    Ok(ResearchTeam {
        team_id: proposal.proposal_id,
        challenge_id: proposal.challenge_id,
        collaboration_compact_hash: sha256_digest(&compact_bytes),
        status: TeamStatus::Forming,
        roster_version: 1,
        members,
        version: 1,
        created_at: now.to_owned(),
        updated_at: now,
    })
}

fn same_materialized_team(existing: &ResearchTeam, expected: &ResearchTeam) -> bool {
    existing.team_id == expected.team_id
        && existing.challenge_id == expected.challenge_id
        && existing.collaboration_compact_hash == expected.collaboration_compact_hash
        && existing.roster_version == expected.roster_version
        && existing.members.len() == expected.members.len()
        && existing
            .members
            .iter()
            .zip(&expected.members)
            .all(|(left, right)| {
                left.participant_slot == right.participant_slot
                    && left.player_id == right.player_id
                    && left.binding_id == right.binding_id
                    && left.agent_id == right.agent_id
                    && left.role == right.role
            })
}

async fn materialize_team_proposal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(proposal_id): Path<Uuid>,
    Json(request): Json<MaterializeTeamProposalRequest>,
) -> Result<(StatusCode, Json<ResearchTeam>), ApiError> {
    const OPERATION: &str = "materialize_team_proposal_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    if request.expected_proposal_version == 0 {
        return Err(ApiError::bad_request(
            "invalid_proposal_version",
            "expected_proposal_version must be positive",
        ));
    }
    let path = format!("/v2/hepta/team-proposals/{proposal_id}/materialize");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();

    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let proposal = memory
            .collaboration
            .team_proposals
            .get(&proposal_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
            })?;
        if !proposal.member_player_ids.contains(&assertion.player_id) {
            return Err(ApiError::forbidden(
                "not_team_proposal_member",
                "only an accepted proposal member can materialize its team",
            ));
        }
        if proposal.version != request.expected_proposal_version {
            return Err(version_conflict(
                "team proposal",
                request.expected_proposal_version,
                proposal.version,
            ));
        }
        let tickets = proposal
            .source_ticket_ids
            .iter()
            .map(|ticket_id| {
                memory
                    .collaboration
                    .tickets
                    .get(ticket_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut bindings = Vec::with_capacity(3);
        for player_id in &proposal.member_player_ids {
            let active = memory
                .bindings
                .values()
                .filter(|binding| {
                    binding.player_id == *player_id && binding.status == AgentBindingStatus::Active
                })
                .cloned()
                .collect::<Vec<_>>();
            if active.len() != 1 {
                return Err(ApiError::conflict(
                    "ambiguous_active_agent_binding",
                    "first-playable materialization requires exactly one active Agent binding per player",
                ));
            }
            bindings.push(active[0].clone());
        }
        let expected = first_playable_team(&proposal, &tickets, &bindings, now)?;
        let (status, team) = if let Some(existing) = memory.teams.get(&proposal_id).cloned() {
            if !same_materialized_team(&existing, &expected) {
                return Err(ApiError::conflict(
                    "research_team_conflict",
                    "proposal team id already exists with a different roster",
                ));
            }
            (StatusCode::OK, existing)
        } else {
            memory.teams.insert(expected.team_id, expected.clone());
            push_memory_event(
                &mut memory,
                OPERATION,
                &request.idempotency_key,
                "hepta.paper_raid.team.materialized.v1",
                expected.team_id,
                expected.version,
                json!({
                    "proposal_id": proposal.proposal_id,
                    "team_id": expected.team_id,
                    "challenge_id": expected.challenge_id,
                    "member_count": expected.members.len(),
                }),
            )?;
            (StatusCode::CREATED, expected)
        };
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            status,
            &team,
        )?;
        *memory_guard = memory;
        return Ok((status, Json(team)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        let team = serde_json::from_value(replay.response).map_err(|error| {
            ApiError::internal(format!("decode materialization replay: {error}"))
        })?;
        return Ok((replay.status, Json(team)));
    }
    let proposal_row =
        sqlx::query("select record_json from hepta_team_proposals where proposal_id=$1 for share")
            .bind(proposal_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
            })?;
    let proposal: TeamProposal = decode_record(proposal_row.get("record_json"), "team proposal")?;
    if !proposal.member_player_ids.contains(&assertion.player_id) {
        return Err(ApiError::forbidden(
            "not_team_proposal_member",
            "only an accepted proposal member can materialize its team",
        ));
    }
    if proposal.version != request.expected_proposal_version {
        return Err(version_conflict(
            "team proposal",
            request.expected_proposal_version,
            proposal.version,
        ));
    }
    let rows =
        sqlx::query("select record_json from hepta_matchmaking_tickets where ticket_id = any($1)")
            .bind(&proposal.source_ticket_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let decoded = rows
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
        .collect::<Result<Vec<MatchmakingTicket>, _>>()?;
    let tickets = proposal
        .source_ticket_ids
        .iter()
        .map(|ticket_id| {
            decoded
                .iter()
                .find(|ticket| ticket.ticket_id == *ticket_id)
                .cloned()
                .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bindings = Vec::with_capacity(3);
    for player_id in &proposal.member_player_ids {
        let rows = sqlx::query(
            "select b.record_json from hepta_agent_bindings b
             join hepta_human_players p on p.player_id=b.player_id
             where b.player_id=$1 and b.status='active' and p.status='active'
             order by b.updated_at desc, b.binding_id",
        )
        .bind(player_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        if rows.len() != 1 {
            return Err(ApiError::conflict(
                "ambiguous_active_agent_binding",
                "first-playable materialization requires exactly one active Agent binding per player",
            ));
        }
        bindings.push(decode_record(rows[0].get("record_json"), "Agent binding")?);
    }
    let expected = first_playable_team(&proposal, &tickets, &bindings, now)?;
    let existing =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for update")
            .bind(expected.team_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let (status, team) =
        if let Some(row) = existing {
            let existing: ResearchTeam = decode_record(row.get("record_json"), "research team")?;
            if !same_materialized_team(&existing, &expected) {
                return Err(ApiError::conflict(
                    "research_team_conflict",
                    "proposal team id already exists with a different roster",
                ));
            }
            (StatusCode::OK, existing)
        } else {
            let record_json = serde_json::to_value(&expected)
                .map_err(|error| ApiError::internal(format!("encode research team: {error}")))?;
            sqlx::query(
                "insert into hepta_research_teams (
                team_id, challenge_id, collaboration_compact_hash, status, roster_version,
                version, record_json, created_at, updated_at
             ) values ($1,$2,$3,'forming',1,1,$4::jsonb,$5,$5)",
            )
            .bind(expected.team_id)
            .bind(expected.challenge_id)
            .bind(&expected.collaboration_compact_hash)
            .bind(record_json)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
            for member in &expected.members {
                sqlx::query(
                    "insert into hepta_research_team_members (
                    team_id, participant_slot, player_id, binding_id, role, joined_at
                 ) values ($1,$2,$3,$4,$5,$6)",
                )
                .bind(expected.team_id)
                .bind(i32::try_from(member.participant_slot).map_err(|_| {
                    ApiError::internal("participant slot exceeds PostgreSQL integer")
                })?)
                .bind(member.player_id)
                .bind(member.binding_id)
                .bind(&member.role)
                .bind(member.joined_at)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            }
            insert_postgres_event(
                &mut tx,
                OPERATION,
                &request.idempotency_key,
                "hepta.paper_raid.team.materialized.v1",
                expected.team_id,
                expected.version,
                json!({
                    "proposal_id": proposal.proposal_id,
                    "team_id": expected.team_id,
                    "challenge_id": expected.challenge_id,
                    "member_count": expected.members.len(),
                }),
            )
            .await?;
            (StatusCode::CREATED, expected)
        };
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(team.team_id),
        status,
        &team,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((status, Json(team)))
}

fn raid_summary(
    team: &ResearchTeam,
    paper: Option<&PaperProject>,
    acceptance_count: usize,
    player_ready: bool,
    player_id: Uuid,
) -> Result<PlayerRaidSummary, ApiError> {
    let member = team
        .members
        .iter()
        .find(|member| member.player_id == player_id)
        .ok_or_else(|| ApiError::internal("visible raid team is missing the current player"))?;
    Ok(PlayerRaidSummary {
        team_id: team.team_id,
        challenge_id: team.challenge_id,
        team_status: team.status.clone(),
        team_version: team.version,
        roster_version: team.roster_version,
        member_count: team.members.len(),
        acceptance_count,
        participant_slot: member.participant_slot,
        role: member.role.clone(),
        player_ready,
        paper: paper.map(|paper| PaperRaidProgress {
            paper_project_id: paper.paper_project_id,
            title: paper.title.clone(),
            phase: paper.phase,
            version: paper.version,
            current_revision_id: paper.current_revision_id,
            release_candidate_revision_id: paper.release_candidate_revision_id,
            updated_at: paper.updated_at.to_owned(),
        }),
        updated_at: paper.map_or_else(
            || team.updated_at.to_owned(),
            |paper| paper.updated_at.to_owned(),
        ),
    })
}

async fn get_player_raid_state(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PlayerRaidState>, ApiError> {
    const PATH: &str = "/v2/hepta/raid-state";
    let assertion =
        require_registered_player_read(&state, &headers, "get_player_raid_state_v1", PATH).await?;
    let mut raids = if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let mut raids = Vec::new();
        for team in memory.teams.values().filter(|team| {
            team.members
                .iter()
                .any(|member| member.player_id == assertion.player_id)
        }) {
            let paper = memory
                .papers
                .values()
                .find(|paper| paper.team_id == team.team_id);
            let acceptances = memory
                .team_acceptances
                .values()
                .filter(|acceptance| {
                    acceptance.team_id == team.team_id && acceptance.superseded_at.is_none()
                })
                .collect::<Vec<_>>();
            raids.push(raid_summary(
                team,
                paper,
                acceptances.len(),
                acceptances
                    .iter()
                    .any(|acceptance| acceptance.player_id == assertion.player_id),
                assertion.player_id,
            )?);
        }
        raids
    } else {
        let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
        let rows = sqlx::query(
            "select t.record_json as team_json, p.record_json as paper_json,
                    (select count(*) from hepta_research_team_member_acceptances a
                     where a.team_id=t.team_id and a.superseded_at is null) as acceptance_count,
                    exists(select 1 from hepta_research_team_member_acceptances a
                           where a.team_id=t.team_id and a.player_id=$1 and a.superseded_at is null) as player_ready
             from hepta_research_team_members m
             join hepta_research_teams t on t.team_id=m.team_id
             left join hepta_paper_projects p on p.team_id=t.team_id
             where m.player_id=$1",
        )
        .bind(assertion.player_id)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?;
        let mut raids = Vec::with_capacity(rows.len());
        for row in rows {
            let team: ResearchTeam = decode_record(row.get("team_json"), "research team")?;
            let paper_json: Option<Value> =
                row.try_get("paper_json").map_err(ApiError::database)?;
            let paper = paper_json
                .map(|value| decode_record(value, "paper project"))
                .transpose()?;
            let acceptance_count: i64 = row.get("acceptance_count");
            raids.push(raid_summary(
                &team,
                paper.as_ref(),
                usize::try_from(acceptance_count)
                    .map_err(|_| ApiError::internal("acceptance count exceeds usize"))?,
                row.get("player_ready"),
                assertion.player_id,
            )?);
        }
        raids
    };
    raids.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.team_id.cmp(&right.team_id))
    });
    let current_raid = raids
        .iter()
        .find(|raid| raid.team_status != TeamStatus::Archived)
        .cloned();
    Ok(Json(PlayerRaidState {
        schema: "hepta.paper_raid.player_raid_state.v1".into(),
        player_id: assertion.player_id,
        current_raid,
        raids,
    }))
}

async fn create_team_proposal_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(proposal_id): Path<Uuid>,
    Json(request): Json<CreateTeamProposalDecisionRequest>,
) -> Result<(StatusCode, Json<TeamProposalDecisionResponse>), ApiError> {
    const OPERATION: &str = "create_team_proposal_decision_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    if request.expected_proposal_version == 0 {
        return Err(ApiError::bad_request(
            "invalid_proposal_version",
            "expected_proposal_version must be positive",
        ));
    }
    let path = format!("/v2/hepta/team-proposals/{proposal_id}/decisions");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden("human_player_not_registered", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let snapshot = memory
            .collaboration
            .team_proposals
            .get(&proposal_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
            })?;
        if !snapshot.member_player_ids.contains(&assertion.player_id) {
            return Err(ApiError::forbidden(
                "not_team_proposal_member",
                "only a proposed member can decide this team proposal",
            ));
        }
        if snapshot.status != TeamProposalStatus::Proposed {
            return Err(ApiError::conflict(
                "team_proposal_not_open",
                "team proposal is no longer awaiting member decisions",
            ));
        }
        if snapshot.version != request.expected_proposal_version {
            return Err(version_conflict(
                "team proposal",
                request.expected_proposal_version,
                snapshot.version,
            ));
        }
        if memory
            .collaboration
            .team_proposal_decisions
            .values()
            .any(|decision| {
                decision.proposal_id == proposal_id && decision.player_id == assertion.player_id
            })
        {
            return Err(ApiError::conflict(
                "team_proposal_decision_exists",
                "player already decided this team proposal",
            ));
        }
        let mut proposal = snapshot.clone();
        proposal.version += 1;
        proposal.updated_at = now;
        let decision = TeamProposalDecision {
            decision_id: request.decision_id,
            proposal_id,
            player_id: assertion.player_id,
            decision: request.decision.clone(),
            proposal_version: proposal.version,
            created_at: now,
        };
        let mut replacement_proposal = None;
        match request.decision {
            TeamProposalDecisionKind::Accept => {
                let previous_accepts = memory
                    .collaboration
                    .team_proposal_decisions
                    .values()
                    .filter(|decision| {
                        decision.proposal_id == proposal_id
                            && decision.decision == TeamProposalDecisionKind::Accept
                    })
                    .count();
                if previous_accepts + 1 == proposal.member_player_ids.len() {
                    proposal.status = TeamProposalStatus::Accepted;
                }
            }
            TeamProposalDecisionKind::Decline => {
                proposal.status = TeamProposalStatus::Declined;
                for ticket_id in &proposal.source_ticket_ids {
                    let ticket = memory
                        .collaboration
                        .tickets
                        .get_mut(ticket_id)
                        .ok_or_else(|| ApiError::internal("team proposal source ticket missing"))?;
                    ticket.status = if ticket.player_id == assertion.player_id {
                        MatchmakingTicketStatus::Cancelled
                    } else {
                        MatchmakingTicketStatus::Queued
                    };
                    ticket.matched_proposal_id = None;
                    ticket.version += 1;
                    ticket.updated_at = now;
                }
                let selected = select_alpha_match(
                    memory
                        .collaboration
                        .tickets
                        .values()
                        .filter(|ticket| ticket.challenge_id == proposal.challenge_id)
                        .cloned(),
                );
                if selected.len() == 3 {
                    let replacement = build_team_proposal(&selected)?;
                    for ticket in &selected {
                        let stored = memory
                            .collaboration
                            .tickets
                            .get_mut(&ticket.ticket_id)
                            .expect("selected replacement ticket exists");
                        stored.status = MatchmakingTicketStatus::Matched;
                        stored.matched_proposal_id = Some(replacement.proposal_id);
                        stored.version += 1;
                        stored.updated_at = now;
                    }
                    memory
                        .collaboration
                        .team_proposals
                        .insert(replacement.proposal_id, replacement.clone());
                    replacement_proposal = Some(replacement);
                }
            }
        }
        if memory
            .collaboration
            .team_proposal_decisions
            .insert(decision.decision_id, decision.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "team_proposal_decision_exists",
                "decision_id already exists",
            ));
        }
        memory
            .collaboration
            .team_proposals
            .insert(proposal_id, proposal.clone());
        let response = TeamProposalDecisionResponse {
            decision: decision.clone(),
            proposal: proposal.clone(),
            replacement_proposal: replacement_proposal.clone(),
        };
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.team_proposal.decision.v1",
            proposal_id,
            proposal.version,
            json!({"proposal_id":proposal_id,"player_id":assertion.player_id,"decision":decision.decision,"status":proposal.status,"replacement_proposal_id":replacement_proposal.map(|item| item.proposal_id)}),
        )
        .expect("team proposal decision event payload is valid JSON");
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &response,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(response)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id=$1 for share")
            .bind(assertion.player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::forbidden("human_player_not_registered", "human player does not exist")
            })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let proposal_row =
        sqlx::query("select record_json from hepta_team_proposals where proposal_id=$1 for update")
            .bind(proposal_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
            })?;
    let mut proposal: TeamProposal =
        decode_record(proposal_row.get("record_json"), "team proposal")?;
    if !proposal.member_player_ids.contains(&assertion.player_id) {
        return Err(ApiError::forbidden(
            "not_team_proposal_member",
            "only a proposed member can decide this team proposal",
        ));
    }
    if proposal.status != TeamProposalStatus::Proposed {
        return Err(ApiError::conflict(
            "team_proposal_not_open",
            "team proposal is no longer awaiting member decisions",
        ));
    }
    if proposal.version != request.expected_proposal_version {
        return Err(version_conflict(
            "team proposal",
            request.expected_proposal_version,
            proposal.version,
        ));
    }
    proposal.version += 1;
    proposal.updated_at = now;
    let decision = TeamProposalDecision {
        decision_id: request.decision_id,
        proposal_id,
        player_id: assertion.player_id,
        decision: request.decision.clone(),
        proposal_version: proposal.version,
        created_at: now,
    };
    sqlx::query(
        "insert into hepta_team_proposal_decisions (
            decision_id,proposal_id,player_id,decision,proposal_version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6::jsonb,$7)",
    )
    .bind(decision.decision_id)
    .bind(proposal_id)
    .bind(decision.player_id)
    .bind(decision.decision.as_str())
    .bind(
        i64::try_from(decision.proposal_version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .bind(
        serde_json::to_value(&decision).map_err(|error| {
            ApiError::internal(format!("encode team proposal decision: {error}"))
        })?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => ApiError::conflict(
            "team_proposal_decision_exists",
            "player or decision_id already decided this proposal",
        ),
        _ => ApiError::database(error),
    })?;
    let mut replacement_proposal = None;
    match request.decision {
        TeamProposalDecisionKind::Accept => {
            let accepts = sqlx::query(
                "select count(*)::bigint as count from hepta_team_proposal_decisions
                 where proposal_id=$1 and decision='accept'",
            )
            .bind(proposal_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .get::<i64, _>("count");
            if usize::try_from(accepts).map_err(|_| ApiError::internal("negative accept count"))?
                == proposal.member_player_ids.len()
            {
                proposal.status = TeamProposalStatus::Accepted;
            }
        }
        TeamProposalDecisionKind::Decline => {
            proposal.status = TeamProposalStatus::Declined;
            sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
                .bind(format!(
                    "hepta-paper-raid-matchmaking:{}",
                    proposal.challenge_id
                ))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            let rows = sqlx::query(
                "select record_json from hepta_matchmaking_tickets
                 where ticket_id=any($1) for update",
            )
            .bind(&proposal.source_ticket_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::database)?;
            if rows.len() != proposal.source_ticket_ids.len() {
                return Err(ApiError::internal(
                    "team proposal source ticket set is incomplete",
                ));
            }
            for row in rows {
                let mut ticket: MatchmakingTicket =
                    decode_record(row.get("record_json"), "matchmaking ticket")?;
                ticket.status = if ticket.player_id == assertion.player_id {
                    MatchmakingTicketStatus::Cancelled
                } else {
                    MatchmakingTicketStatus::Queued
                };
                ticket.matched_proposal_id = None;
                ticket.version += 1;
                ticket.updated_at = now;
                sqlx::query(
                    "update hepta_matchmaking_tickets set status=$1,matched_proposal_id=null,
                     version=$2,record_json=$3::jsonb,updated_at=$4 where ticket_id=$5",
                )
                .bind(ticket.status.as_str())
                .bind(
                    i64::try_from(ticket.version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .bind(serde_json::to_value(&ticket).map_err(|error| {
                    ApiError::internal(format!("encode requeued ticket: {error}"))
                })?)
                .bind(now)
                .bind(ticket.ticket_id)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            }
            let queue_rows = sqlx::query(
                "select record_json from hepta_matchmaking_tickets
                 where challenge_id=$1 and requested_team_size=3 and status='queued'
                 order by created_at,ticket_id for update",
            )
            .bind(proposal.challenge_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::database)?;
            let selected = select_alpha_match(
                queue_rows
                    .into_iter()
                    .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter(),
            );
            if selected.len() == 3 {
                let replacement = build_team_proposal(&selected)?;
                sqlx::query(
                    "insert into hepta_team_proposals (
                        proposal_id,challenge_id,requested_team_size,deterministic_match_key,status,
                        member_player_ids,source_ticket_ids,version,record_json,created_at,updated_at
                     ) values ($1,$2,3,$3,'proposed',$4,$5,1,$6::jsonb,$7,$7)",
                )
                .bind(replacement.proposal_id)
                .bind(replacement.challenge_id)
                .bind(&replacement.deterministic_match_key)
                .bind(&replacement.member_player_ids)
                .bind(&replacement.source_ticket_ids)
                .bind(serde_json::to_value(&replacement).map_err(|error| {
                    ApiError::internal(format!("encode replacement team proposal: {error}"))
                })?)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
                for mut ticket in selected {
                    ticket.status = MatchmakingTicketStatus::Matched;
                    ticket.matched_proposal_id = Some(replacement.proposal_id);
                    ticket.version += 1;
                    ticket.updated_at = now;
                    sqlx::query(
                        "update hepta_matchmaking_tickets set status='matched',matched_proposal_id=$1,
                         version=$2,record_json=$3::jsonb,updated_at=$4
                         where ticket_id=$5 and status='queued'",
                    )
                    .bind(replacement.proposal_id)
                    .bind(i64::try_from(ticket.version).map_err(|_| ApiError::internal("ticket version overflow"))?)
                    .bind(serde_json::to_value(&ticket).map_err(|error| {
                        ApiError::internal(format!("encode rematched ticket: {error}"))
                    })?)
                    .bind(now)
                    .bind(ticket.ticket_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(ApiError::database)?;
                }
                replacement_proposal = Some(replacement);
            }
        }
    }
    let updated = sqlx::query(
        "update hepta_team_proposals set status=$1,version=$2,record_json=$3::jsonb,updated_at=$4
         where proposal_id=$5 and status='proposed' and version=$6",
    )
    .bind(proposal.status.as_str())
    .bind(
        i64::try_from(proposal.version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .bind(
        serde_json::to_value(&proposal).map_err(|error| {
            ApiError::internal(format!("encode decided team proposal: {error}"))
        })?,
    )
    .bind(now)
    .bind(proposal_id)
    .bind(
        i64::try_from(request.expected_proposal_version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "team_proposal_decision_race",
            "team proposal changed before decision committed",
        ));
    }
    let response = TeamProposalDecisionResponse {
        decision: decision.clone(),
        proposal: proposal.clone(),
        replacement_proposal: replacement_proposal.clone(),
    };
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.team_proposal.decision.v1",
        proposal_id,
        proposal.version,
        json!({"proposal_id":proposal_id,"player_id":assertion.player_id,"decision":decision.decision,"status":proposal.status,"replacement_proposal_id":replacement_proposal.map(|item| item.proposal_id)}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(decision.decision_id),
        StatusCode::CREATED,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(response)))
}

fn validate_artifact_manifest_request(
    request: &CreateArtifactManifestRequest,
) -> Result<(u64, String), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    let bundle = &request.source_bundle;
    if bundle.schema != SOURCE_ARTIFACT_BUNDLE_SCHEMA_V1
        || bundle.artifact_root.algorithm != "sha256-canonical-manifest-v1"
        || bundle.artifact_root.digest_file != "artifact-bundle.v1.sha256"
    {
        return Err(ApiError::bad_request(
            "source_bundle_schema_mismatch",
            "source bundle must use the exact pinned Integration artifact-bundle.v1 root contract",
        ));
    }
    if bundle.hepta_binding_status != "unbound" || bundle.human_authority_materialized {
        return Err(ApiError::bad_request(
            "source_bundle_claims_human_authority",
            "neutral source bundle must remain unbound and contain no materialized human authority",
        ));
    }
    validate_collaboration_text("bundle_id", &bundle.bundle_id)?;
    validate_collaboration_text("challenge_id", &bundle.challenge_id)?;
    DateTime::parse_from_rfc3339(&bundle.created_at).map_err(|_| {
        ApiError::bad_request(
            "invalid_bundle_created_at",
            "source bundle created_at must be RFC3339",
        )
    })?;
    if bundle.objects.is_empty()
        || bundle.objects.len() > 4_096
        || bundle.object_count != u64::try_from(bundle.objects.len()).expect("4096 fits u64")
    {
        return Err(ApiError::bad_request(
            "invalid_artifact_object_count",
            "object_count must equal the 1-4096 exact neutral object descriptors",
        ));
    }
    if bundle.required_run_ids.is_empty() || bundle.required_run_ids.len() > 256 {
        return Err(ApiError::bad_request(
            "invalid_required_runs",
            "source bundle requires 1-256 distinct required_run_ids",
        ));
    }
    let mut run_ids = HashSet::new();
    for run_id in &bundle.required_run_ids {
        validate_collaboration_text("required_run_id", run_id)?;
        if !run_ids.insert(run_id) {
            return Err(ApiError::bad_request(
                "duplicate_required_run",
                "required_run_ids must be unique",
            ));
        }
    }
    let mut paths = HashSet::new();
    let mut digests = HashSet::new();
    let mut total_size = 0_u64;
    let mut previous_path: Option<&str> = None;
    for object in &bundle.objects {
        validate_logical_path(&object.logical_path)?;
        if object.media_type.is_empty()
            || object.media_type.len() > 200
            || object
                .media_type
                .bytes()
                .any(|byte| byte.is_ascii_control())
        {
            return Err(ApiError::bad_request(
                "invalid_artifact_media_type",
                "neutral object media_type must be nonempty, bounded, and contain no controls",
            ));
        }
        validate_collaboration_text("artifact_role", &object.role)?;
        validate_raw_sha256("object_sha256", &object.sha256)?;
        if previous_path.is_some_and(|previous| previous >= object.logical_path.as_str())
            || !paths.insert(&object.logical_path)
        {
            return Err(ApiError::bad_request(
                "noncanonical_artifact_object_order",
                "neutral objects must have unique logical_path values in strict ascending order",
            ));
        }
        previous_path = Some(&object.logical_path);
        digests.insert(object.sha256.as_str());
        if object
            .dependencies
            .iter()
            .any(|digest| validate_raw_sha256("dependency", digest).is_err())
            || object
                .dependencies
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(ApiError::bad_request(
                "noncanonical_artifact_dependencies",
                "object dependencies must be unique lowercase SHA-256 values in ascending order",
            ));
        }
        total_size = total_size.checked_add(object.size).ok_or_else(|| {
            ApiError::bad_request(
                "artifact_size_overflow",
                "artifact total size overflows u64",
            )
        })?;
    }
    if bundle
        .objects
        .iter()
        .flat_map(|object| object.dependencies.iter())
        .any(|dependency| !digests.contains(dependency.as_str()))
    {
        return Err(ApiError::bad_request(
            "unknown_artifact_dependency",
            "every dependency must name an object in the same neutral manifest",
        ));
    }
    if total_size > 1_099_511_627_776 {
        return Err(ApiError::bad_request(
            "artifact_bundle_too_large",
            "artifact metadata declares more than one TiB",
        ));
    }
    if request.storage_locations.len() != bundle.objects.len() {
        return Err(ApiError::bad_request(
            "artifact_storage_mapping_incomplete",
            "storage_locations must map every neutral object exactly once",
        ));
    }
    let object_by_path: HashMap<_, _> = bundle
        .objects
        .iter()
        .map(|object| (object.logical_path.as_str(), object))
        .collect();
    let mut location_paths = HashSet::new();
    for location in &request.storage_locations {
        validate_logical_path(&location.logical_path)?;
        validate_raw_sha256("storage_sha256", &location.sha256)?;
        let object = object_by_path
            .get(location.logical_path.as_str())
            .ok_or_else(|| {
                ApiError::bad_request(
                    "artifact_storage_mapping_unknown",
                    "storage location names a path absent from the neutral manifest",
                )
            })?;
        if !location_paths.insert(location.logical_path.as_str())
            || object.sha256 != location.sha256
        {
            return Err(ApiError::bad_request(
                "artifact_storage_mapping_mismatch",
                "storage path/digest mappings must be unique and exactly match the neutral manifest",
            ));
        }
        validate_artifact_uri(&location.uri, &format!("sha256:{}", location.sha256))?;
    }
    validate_raw_sha256(
        "expected_source_manifest_sha256",
        &request.expected_source_manifest_sha256,
    )?;
    let source_sha256 = neutral_bundle_sha256(bundle)?;
    if source_sha256 != request.expected_source_manifest_sha256 {
        return Err(ApiError::bad_request(
            "source_manifest_hash_mismatch",
            "expected source digest does not match independently canonicalized bundle bytes",
        ));
    }
    Ok((total_size, source_sha256))
}

fn validate_raw_sha256(field: &'static str, value: &str) -> Result<(), ApiError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err(ApiError::bad_request(
            "invalid_raw_sha256",
            format!("{field} must be 64 lowercase hexadecimal characters"),
        ));
    }
    Ok(())
}

fn neutral_bundle_sha256(bundle: &NeutralArtifactBundleV1) -> Result<String, ApiError> {
    let mut bytes = crate::paper_raid_contracts::canonical_json_bytes(bundle).map_err(|error| {
        ApiError::internal(format!("canonicalize neutral artifact bundle: {error}"))
    })?;
    bytes.push(b'\n');
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn validate_artifact_challenge_id(
    source_challenge_id: &str,
    paper_challenge_id: Uuid,
) -> Result<(), ApiError> {
    if source_challenge_id != paper_challenge_id.to_string() {
        return Err(ApiError::bad_request(
            "artifact_challenge_mismatch",
            "neutral bundle challenge_id must exactly equal PaperProject.challenge_id",
        ));
    }
    Ok(())
}

fn unique_artifact_role_object(
    manifest: &ArtifactManifest,
    role: &'static str,
) -> Result<NeutralArtifactObjectV1, ApiError> {
    let matches = manifest
        .objects
        .iter()
        .filter(|object| object.role == role)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Err(ApiError::conflict(
            "paper_revision_artifact_role_missing",
            format!("artifact bundle requires exactly one {role} object"),
        ));
    }
    if matches.len() != 1 {
        return Err(ApiError::conflict(
            "paper_revision_artifact_role_ambiguous",
            format!("artifact bundle contains more than one {role} object"),
        ));
    }
    Ok(matches[0].clone())
}

fn resolve_paper_revision_artifact_binding(
    revision_id: Uuid,
    paper_id: Uuid,
    manifest: &ArtifactManifest,
    request: &CreatePaperRevisionRequest,
) -> Result<PaperRevisionArtifactBinding, ApiError> {
    if manifest.paper_project_id != paper_id {
        return Err(ApiError::conflict(
            "paper_revision_artifact_cross_paper",
            "artifact bundle belongs to a different paper project",
        ));
    }
    if request.artifact_manifest_hash != manifest.manifest_hash {
        return Err(ApiError::conflict(
            "paper_revision_artifact_root_mismatch",
            "artifact_manifest_hash must equal the verified neutral bundle root",
        ));
    }
    let source = unique_artifact_role_object(manifest, "paper_source")?;
    let bibliography = unique_artifact_role_object(manifest, "bibliography")?;
    let claim_graph = unique_artifact_role_object(manifest, "claim_evidence_graph")?;
    let expected_source = format!("sha256:{}", source.sha256);
    let expected_bibliography = format!("sha256:{}", bibliography.sha256);
    let expected_claim_graph = format!("sha256:{}", claim_graph.sha256);
    if request.source_manifest_hash != expected_source
        || request.bibliography_hash != expected_bibliography
        || request.claim_evidence_graph_hash != expected_claim_graph
    {
        return Err(ApiError::conflict(
            "paper_revision_artifact_descriptor_mismatch",
            "source, bibliography, and claim-evidence hashes must exactly match the unique typed objects in the same verified bundle",
        ));
    }
    Ok(PaperRevisionArtifactBinding {
        revision_id,
        paper_project_id: paper_id,
        manifest_id: manifest.manifest_id,
        artifact_manifest_hash: manifest.manifest_hash.clone(),
        source_logical_path: source.logical_path,
        source_manifest_hash: expected_source,
        bibliography_logical_path: bibliography.logical_path,
        bibliography_hash: expected_bibliography,
        claim_evidence_graph_logical_path: claim_graph.logical_path,
        claim_evidence_graph_hash: expected_claim_graph,
        created_at: Utc::now(),
    })
}

pub(super) fn resolve_revision_artifact_binding_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    request: &CreatePaperRevisionRequest,
) -> Result<PaperRevisionArtifactBinding, ApiError> {
    let manifest = memory
        .collaboration
        .artifact_manifests
        .values()
        .filter(|manifest| manifest.manifest_hash == request.artifact_manifest_hash)
        .find(|manifest| manifest.paper_project_id == paper_id)
        .or_else(|| {
            memory
                .collaboration
                .artifact_manifests
                .values()
                .find(|manifest| manifest.manifest_hash == request.artifact_manifest_hash)
        })
        .ok_or_else(|| {
            ApiError::conflict(
                "paper_revision_artifact_manifest_unknown",
                "artifact_manifest_hash does not name a verified ArtifactManifest",
            )
        })?;
    resolve_paper_revision_artifact_binding(request.revision_id, paper_id, manifest, request)
}

pub(super) fn store_revision_artifact_binding_memory(
    memory: &mut PaperRaidMemory,
    binding: PaperRevisionArtifactBinding,
) -> Result<(), ApiError> {
    if memory
        .collaboration
        .revision_artifact_bindings
        .insert(binding.revision_id, binding)
        .is_some()
    {
        return Err(ApiError::conflict(
            "paper_revision_artifact_binding_exists",
            "paper revision already has an artifact binding",
        ));
    }
    Ok(())
}

pub(super) async fn resolve_revision_artifact_binding_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    request: &CreatePaperRevisionRequest,
) -> Result<PaperRevisionArtifactBinding, ApiError> {
    let rows = sqlx::query(
        "select record_json from hepta_artifact_manifests where manifest_hash=$1 for share",
    )
    .bind(&request.artifact_manifest_hash)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let manifests = rows
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "artifact manifest"))
        .collect::<Result<Vec<ArtifactManifest>, ApiError>>()?;
    let manifest = manifests
        .iter()
        .find(|manifest| manifest.paper_project_id == paper_id)
        .or_else(|| manifests.first())
        .ok_or_else(|| {
            ApiError::conflict(
                "paper_revision_artifact_manifest_unknown",
                "artifact_manifest_hash does not name a verified ArtifactManifest",
            )
        })?;
    resolve_paper_revision_artifact_binding(request.revision_id, paper_id, manifest, request)
}

pub(super) async fn store_revision_artifact_binding_postgres(
    tx: &mut Transaction<'_, Postgres>,
    binding: &PaperRevisionArtifactBinding,
) -> Result<(), ApiError> {
    sqlx::query(
        "insert into hepta_paper_revision_artifact_bindings (
            revision_id,paper_project_id,manifest_id,artifact_manifest_hash,
            source_logical_path,source_manifest_hash,bibliography_logical_path,
            bibliography_hash,claim_evidence_graph_logical_path,
            claim_evidence_graph_hash,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,$12)",
    )
    .bind(binding.revision_id)
    .bind(binding.paper_project_id)
    .bind(binding.manifest_id)
    .bind(&binding.artifact_manifest_hash)
    .bind(&binding.source_logical_path)
    .bind(&binding.source_manifest_hash)
    .bind(&binding.bibliography_logical_path)
    .bind(&binding.bibliography_hash)
    .bind(&binding.claim_evidence_graph_logical_path)
    .bind(&binding.claim_evidence_graph_hash)
    .bind(serde_json::to_value(binding).map_err(|error| {
        ApiError::internal(format!("encode paper revision artifact binding: {error}"))
    })?)
    .bind(binding.created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => ApiError::conflict(
            "paper_revision_artifact_binding_exists",
            "paper revision already has an artifact binding",
        ),
        _ => ApiError::database(error),
    })?;
    Ok(())
}

async fn create_artifact_manifest(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateArtifactManifestRequest>,
) -> Result<(StatusCode, Json<ArtifactManifest>), ApiError> {
    const OPERATION: &str = "create_artifact_manifest_v3";
    let (total_size, source_manifest_sha256) = validate_artifact_manifest_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/artifact-manifests");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();
    let manifest = ArtifactManifest {
        manifest_id: request.manifest_id,
        paper_project_id: paper_id,
        binding_schema: ARTIFACT_MANIFEST_BINDING_SCHEMA_V1.to_string(),
        source_bundle_schema: request.source_bundle.schema.clone(),
        source_bundle_id: request.source_bundle.bundle_id.clone(),
        source_challenge_id: request.source_bundle.challenge_id.clone(),
        source_created_at: request.source_bundle.created_at.clone(),
        source_manifest_sha256: source_manifest_sha256.clone(),
        manifest_hash: format!("sha256:{source_manifest_sha256}"),
        object_count: request.source_bundle.object_count,
        objects: request.source_bundle.objects.clone(),
        required_run_ids: request.source_bundle.required_run_ids.clone(),
        storage_locations: request.storage_locations.clone(),
        total_size_bytes: total_size,
        version: 1,
        created_at: now,
    };
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::ArtifactManifest)?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        validate_artifact_challenge_id(&manifest.source_challenge_id, paper.challenge_id)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if memory
            .collaboration
            .artifact_manifests
            .values()
            .any(|existing| {
                existing.paper_project_id == paper_id
                    && existing.manifest_hash == manifest.manifest_hash
            })
            || memory
                .collaboration
                .artifact_manifests
                .insert(manifest.manifest_id, manifest.clone())
                .is_some()
        {
            return Err(ApiError::conflict(
                "artifact_manifest_exists",
                "manifest_id or paper-scoped manifest_hash already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.artifact_manifest.created.v1",
            paper_id,
            manifest.manifest_id,
            manifest.version,
            json!({
                "manifest_id":manifest.manifest_id,
                "manifest_hash":manifest.manifest_hash,
                "binding_schema":manifest.binding_schema,
                "source_bundle_schema":manifest.source_bundle_schema,
                "source_manifest_sha256":manifest.source_manifest_sha256,
                "object_count":manifest.object_count,
            }),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &manifest,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(manifest)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return Ok((
            replay.status,
            Json(serde_json::from_value(replay.response).map_err(|error| {
                ApiError::internal(format!("decode artifact manifest replay: {error}"))
            })?),
        ));
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::ArtifactManifest)?;
    if paper.version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper.version,
        ));
    }
    validate_artifact_challenge_id(&manifest.source_challenge_id, paper.challenge_id)?;
    sqlx::query(
        "insert into hepta_artifact_manifests (
            manifest_id,paper_project_id,binding_schema,source_bundle_schema,
            source_bundle_id,source_challenge_id,source_created_at,source_manifest_sha256,
            manifest_hash,object_count,storage_location_count,total_size_bytes,
            version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,1,$13::jsonb,$14)",
    )
    .bind(manifest.manifest_id)
    .bind(paper_id)
    .bind(&manifest.binding_schema)
    .bind(&manifest.source_bundle_schema)
    .bind(&manifest.source_bundle_id)
    .bind(&manifest.source_challenge_id)
    .bind(&manifest.source_created_at)
    .bind(&manifest.source_manifest_sha256)
    .bind(&manifest.manifest_hash)
    .bind(i32::try_from(manifest.object_count).map_err(|_| {
        ApiError::bad_request(
            "artifact_object_count_overflow",
            "object count exceeds PostgreSQL integer",
        )
    })?)
    .bind(i32::try_from(manifest.storage_locations.len()).expect("4096 locations fit i32"))
    .bind(i64::try_from(manifest.total_size_bytes).map_err(|_| {
        ApiError::bad_request(
            "artifact_size_overflow",
            "artifact size exceeds PostgreSQL bigint",
        )
    })?)
    .bind(
        serde_json::to_value(&manifest)
            .map_err(|error| ApiError::internal(format!("encode artifact manifest: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => ApiError::conflict(
            "artifact_manifest_exists",
            "artifact manifest already exists",
        ),
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.artifact_manifest.created.v1",
        paper_id,
        manifest.manifest_id,
        manifest.version,
        json!({
            "manifest_id":manifest.manifest_id,
            "manifest_hash":manifest.manifest_hash,
                "binding_schema":manifest.binding_schema,
                "source_bundle_schema":manifest.source_bundle_schema,
                "source_manifest_sha256":manifest.source_manifest_sha256,
                "object_count":manifest.object_count,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(manifest.manifest_id),
        StatusCode::CREATED,
        &manifest,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(manifest)))
}

struct HumanVerificationInput<'a> {
    paper_id: Uuid,
    assertion: &'a ConsumerUserAssertionClaimV2,
    record_kind: &'a str,
    record_id: Uuid,
    source_identifier: String,
    source_hash: &'a str,
    locator: &'a str,
    license: &'a str,
    key_id: &'a str,
    public_key: &'a str,
    public_key_hash: &'a str,
    signed_at_unix: i64,
    signature: &'a str,
}

fn validate_human_verification(
    input: HumanVerificationInput<'_>,
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ApiError> {
    signed_time(input.signed_at_unix)?;
    validate_digest_v2("source_hash", input.source_hash)?;
    validate_collaboration_text("locator", input.locator)?;
    validate_collaboration_text("license", input.license)?;
    let signing = HumanEvidenceVerificationSigningV1 {
        schema: HUMAN_EVIDENCE_VERIFICATION_V1.to_string(),
        verification_id: input.record_id,
        paper_project_id: input.paper_id,
        record_kind: input.record_kind.to_string(),
        record_id: input.record_id,
        source_identifier: input.source_identifier,
        source_hash: input.source_hash.to_string(),
        locator: input.locator.to_string(),
        license: input.license.to_string(),
        player_id: input.assertion.player_id,
        signing_key_id: input.key_id.to_string(),
        signing_public_key_hash: input.public_key_hash.to_string(),
        signed_at_unix: input.signed_at_unix,
    };
    let bytes = human_evidence_verification_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_human_verification", message))?;
    verifying_key
        .verify(&bytes, &decode_signature(input.signature)?)
        .map_err(|_| {
            ApiError::forbidden(
                "human_verification_signature_failed",
                "human evidence verification signature failed",
            )
        })?;
    if crate::paper_raid_contracts::sha256_digest(
        &crate::decode_verifying_key(input.public_key)?.to_bytes(),
    ) != input.public_key_hash
    {
        return Err(ApiError::bad_request(
            "human_key_hash_mismatch",
            "human verification public-key hash does not match key",
        ));
    }
    Ok(())
}

async fn create_evidence_card(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateEvidenceCardRequest>,
) -> Result<(StatusCode, Json<EvidenceCard>), ApiError> {
    const OPERATION: &str = "create_evidence_card_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_public_source_uri(&request.source_uri)?;
    let path = format!("/v2/hepta/papers/{paper_id}/evidence-cards");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let created_at = Utc::now();
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Evidence)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let verifying_key = active_human_key_memory(
            &memory,
            assertion.player_id,
            &request.verification_key_id,
            &request.verification_public_key,
            &request.verification_public_key_hash,
            request.signed_at_unix,
        )?;
        validate_human_verification(
            HumanVerificationInput {
                paper_id,
                assertion: &assertion,
                record_kind: "evidence_card",
                record_id: request.evidence_card_id,
                source_identifier: evidence_source_identifier(&request.source_uri),
                source_hash: &request.source_hash,
                locator: &request.locator,
                license: &request.license,
                key_id: &request.verification_key_id,
                public_key: &request.verification_public_key,
                public_key_hash: &request.verification_public_key_hash,
                signed_at_unix: request.signed_at_unix,
                signature: &request.verification_signature,
            },
            &verifying_key,
        )?;
        let card = EvidenceCard {
            evidence_card_id: request.evidence_card_id,
            paper_project_id: paper_id,
            source_uri: request.source_uri.clone(),
            source_hash: request.source_hash.clone(),
            locator: request.locator.clone(),
            license: request.license.clone(),
            verified_by_player_id: assertion.player_id,
            verification_key_id: request.verification_key_id.clone(),
            verification_public_key: request.verification_public_key.clone(),
            verification_public_key_hash: request.verification_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            verification_signature: request.verification_signature.clone(),
            version: 1,
            created_at,
        };
        if memory
            .collaboration
            .evidence_cards
            .insert(card.evidence_card_id, card.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "evidence_card_exists",
                "evidence_card_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.evidence_card.created.v1",
            paper_id,
            card.evidence_card_id,
            1,
            json!({"evidence_card_id":card.evidence_card_id,"source_hash":card.source_hash}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &card,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(card)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return Ok((
            replay.status,
            Json(serde_json::from_value(replay.response).map_err(|error| {
                ApiError::internal(format!("decode evidence card replay: {error}"))
            })?),
        ));
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Evidence)?;
    let verifying_key = active_human_key_postgres(
        &mut tx,
        assertion.player_id,
        &request.verification_key_id,
        &request.verification_public_key,
        &request.verification_public_key_hash,
        request.signed_at_unix,
    )
    .await?;
    validate_human_verification(
        HumanVerificationInput {
            paper_id,
            assertion: &assertion,
            record_kind: "evidence_card",
            record_id: request.evidence_card_id,
            source_identifier: evidence_source_identifier(&request.source_uri),
            source_hash: &request.source_hash,
            locator: &request.locator,
            license: &request.license,
            key_id: &request.verification_key_id,
            public_key: &request.verification_public_key,
            public_key_hash: &request.verification_public_key_hash,
            signed_at_unix: request.signed_at_unix,
            signature: &request.verification_signature,
        },
        &verifying_key,
    )?;
    let card = EvidenceCard {
        evidence_card_id: request.evidence_card_id,
        paper_project_id: paper_id,
        source_uri: request.source_uri.clone(),
        source_hash: request.source_hash.clone(),
        locator: request.locator.clone(),
        license: request.license.clone(),
        verified_by_player_id: assertion.player_id,
        verification_key_id: request.verification_key_id.clone(),
        verification_public_key: request.verification_public_key.clone(),
        verification_public_key_hash: request.verification_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        verification_signature: request.verification_signature.clone(),
        version: 1,
        created_at,
    };
    sqlx::query(
        "insert into hepta_evidence_cards (
            evidence_card_id, paper_project_id, source_uri, source_hash, locator,
            license, verified_by_player_id, verification_key_id,
            verification_signature, version, record_json, created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,1,$10::jsonb,$11)",
    )
    .bind(card.evidence_card_id)
    .bind(paper_id)
    .bind(&card.source_uri)
    .bind(&card.source_hash)
    .bind(&card.locator)
    .bind(&card.license)
    .bind(card.verified_by_player_id)
    .bind(&card.verification_key_id)
    .bind(&card.verification_signature)
    .bind(
        serde_json::to_value(&card)
            .map_err(|error| ApiError::internal(format!("encode evidence card: {error}")))?,
    )
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("evidence_card_exists", "evidence card already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.evidence_card.created.v1",
        paper_id,
        card.evidence_card_id,
        1,
        json!({"evidence_card_id":card.evidence_card_id,"source_hash":card.source_hash}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(card.evidence_card_id),
        StatusCode::CREATED,
        &card,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(card)))
}

fn validate_distinct_ids(
    field: &'static str,
    ids: &[Uuid],
    allow_empty: bool,
) -> Result<(), ApiError> {
    if (!allow_empty && ids.is_empty()) || ids.len() > 256 {
        return Err(ApiError::bad_request(
            "invalid_reference_count",
            format!(
                "{field} must contain {} to 256 identifiers",
                if allow_empty { 0 } else { 1 }
            ),
        ));
    }
    if ids.iter().copied().collect::<HashSet<_>>().len() != ids.len() {
        return Err(ApiError::bad_request(
            "duplicate_reference",
            format!("{field} must not contain duplicate identifiers"),
        ));
    }
    Ok(())
}

async fn require_scoped_ids_postgres(
    tx: &mut Transaction<'_, Postgres>,
    table: &'static str,
    id_column: &'static str,
    paper_id: Uuid,
    ids: &[Uuid],
    field: &'static str,
) -> Result<(), ApiError> {
    if ids.is_empty() {
        return Ok(());
    }
    let statement = format!(
        "select {id_column}::text as scoped_id from {table} \
         where {id_column} = any($1) and paper_project_id = $2 for share"
    );
    let rows = sqlx::query(&statement)
        .bind(ids)
        .bind(paper_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    if rows.len() != ids.len() {
        return Err(ApiError::bad_request(
            "cross_paper_or_missing_reference",
            format!("{field} contains a missing or cross-paper reference"),
        ));
    }
    Ok(())
}

fn validate_citation_request(request: &CreateCitationRecordRequest) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    if request.doi.is_none() && request.canonical_url.is_none() {
        return Err(ApiError::bad_request(
            "citation_locator_required",
            "citation requires a DOI or canonical_url",
        ));
    }
    if let Some(doi) = &request.doi {
        validate_collaboration_text("doi", doi)?;
        if !doi.starts_with("10.")
            || !doi.contains('/')
            || doi.bytes().any(|byte| byte.is_ascii_whitespace())
        {
            return Err(ApiError::bad_request(
                "invalid_doi",
                "DOI must use the canonical 10.<registrant>/<suffix> form",
            ));
        }
    }
    if let Some(url) = &request.canonical_url {
        validate_public_source_uri(url)?;
    }
    validate_collaboration_text(
        "citation_source_identifier",
        &citation_source_identifier(request.doi.as_deref(), request.canonical_url.as_deref()),
    )?;
    Ok(())
}

async fn create_citation_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateCitationRecordRequest>,
) -> Result<(StatusCode, Json<CitationRecord>), ApiError> {
    const OPERATION: &str = "create_citation_record_v3";
    validate_citation_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/citations");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let created_at = Utc::now();
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Citation)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let evidence = memory
            .collaboration
            .evidence_cards
            .get(&request.evidence_card_id)
            .filter(|record| record.paper_project_id == paper_id)
            .ok_or_else(|| {
                ApiError::bad_request(
                    "cross_paper_or_missing_evidence",
                    "citation evidence card is missing or belongs to another paper",
                )
            })?;
        if evidence.source_hash != request.source_hash
            || evidence.locator != request.locator
            || evidence.license != request.license
        {
            return Err(ApiError::bad_request(
                "citation_evidence_mismatch",
                "citation must preserve the verified evidence hash, locator, and license",
            ));
        }
        let verifying_key = active_human_key_memory(
            &memory,
            assertion.player_id,
            &request.verification_key_id,
            &request.verification_public_key,
            &request.verification_public_key_hash,
            request.signed_at_unix,
        )?;
        validate_human_verification(
            HumanVerificationInput {
                paper_id,
                assertion: &assertion,
                record_kind: "citation",
                record_id: request.citation_id,
                source_identifier: citation_source_identifier(
                    request.doi.as_deref(),
                    request.canonical_url.as_deref(),
                ),
                source_hash: &request.source_hash,
                locator: &request.locator,
                license: &request.license,
                key_id: &request.verification_key_id,
                public_key: &request.verification_public_key,
                public_key_hash: &request.verification_public_key_hash,
                signed_at_unix: request.signed_at_unix,
                signature: &request.verification_signature,
            },
            &verifying_key,
        )?;
        let record = CitationRecord {
            citation_id: request.citation_id,
            paper_project_id: paper_id,
            evidence_card_id: request.evidence_card_id,
            doi: request.doi.clone(),
            canonical_url: request.canonical_url.clone(),
            source_hash: request.source_hash.clone(),
            locator: request.locator.clone(),
            license: request.license.clone(),
            verified_by_player_id: assertion.player_id,
            verification_key_id: request.verification_key_id.clone(),
            verification_public_key: request.verification_public_key.clone(),
            verification_public_key_hash: request.verification_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            verification_signature: request.verification_signature.clone(),
            version: 1,
            created_at,
        };
        if memory
            .collaboration
            .citations
            .insert(record.citation_id, record.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "citation_exists",
                "citation_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.citation.created.v1",
            paper_id,
            record.citation_id,
            1,
            json!({"citation_id":record.citation_id,"evidence_card_id":record.evidence_card_id}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &record,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(record)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Citation)?;
    let evidence_row = sqlx::query(
        "select record_json from hepta_evidence_cards
         where evidence_card_id=$1 and paper_project_id=$2 for share",
    )
    .bind(request.evidence_card_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::bad_request(
            "cross_paper_or_missing_evidence",
            "citation evidence card is missing or belongs to another paper",
        )
    })?;
    let evidence: EvidenceCard = decode_record(evidence_row.get("record_json"), "evidence card")?;
    if evidence.source_hash != request.source_hash
        || evidence.locator != request.locator
        || evidence.license != request.license
    {
        return Err(ApiError::bad_request(
            "citation_evidence_mismatch",
            "citation must preserve the verified evidence hash, locator, and license",
        ));
    }
    let verifying_key = active_human_key_postgres(
        &mut tx,
        assertion.player_id,
        &request.verification_key_id,
        &request.verification_public_key,
        &request.verification_public_key_hash,
        request.signed_at_unix,
    )
    .await?;
    validate_human_verification(
        HumanVerificationInput {
            paper_id,
            assertion: &assertion,
            record_kind: "citation",
            record_id: request.citation_id,
            source_identifier: citation_source_identifier(
                request.doi.as_deref(),
                request.canonical_url.as_deref(),
            ),
            source_hash: &request.source_hash,
            locator: &request.locator,
            license: &request.license,
            key_id: &request.verification_key_id,
            public_key: &request.verification_public_key,
            public_key_hash: &request.verification_public_key_hash,
            signed_at_unix: request.signed_at_unix,
            signature: &request.verification_signature,
        },
        &verifying_key,
    )?;
    let record = CitationRecord {
        citation_id: request.citation_id,
        paper_project_id: paper_id,
        evidence_card_id: request.evidence_card_id,
        doi: request.doi.clone(),
        canonical_url: request.canonical_url.clone(),
        source_hash: request.source_hash.clone(),
        locator: request.locator.clone(),
        license: request.license.clone(),
        verified_by_player_id: assertion.player_id,
        verification_key_id: request.verification_key_id.clone(),
        verification_public_key: request.verification_public_key.clone(),
        verification_public_key_hash: request.verification_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        verification_signature: request.verification_signature.clone(),
        version: 1,
        created_at,
    };
    sqlx::query(
        "insert into hepta_citation_records (
            citation_id,paper_project_id,evidence_card_id,doi,canonical_url,
            source_hash,locator,license,verified_by_player_id,verification_key_id,
            verification_signature,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,1,$12::jsonb,$13)",
    )
    .bind(record.citation_id)
    .bind(paper_id)
    .bind(record.evidence_card_id)
    .bind(&record.doi)
    .bind(&record.canonical_url)
    .bind(&record.source_hash)
    .bind(&record.locator)
    .bind(&record.license)
    .bind(record.verified_by_player_id)
    .bind(&record.verification_key_id)
    .bind(&record.verification_signature)
    .bind(
        serde_json::to_value(&record)
            .map_err(|error| ApiError::internal(format!("encode citation record: {error}")))?,
    )
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("citation_exists", "citation_id already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.citation.created.v1",
        paper_id,
        record.citation_id,
        1,
        json!({"citation_id":record.citation_id,"evidence_card_id":record.evidence_card_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(record.citation_id),
        StatusCode::CREATED,
        &record,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

fn validate_experiment_plan_request(request: &CreateExperimentPlanRequest) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("protocol_snapshot_hash", &request.protocol_snapshot_hash)?;
    validate_digest_v2("seed_policy_hash", &request.seed_policy_hash)?;
    validate_digest_v2("stopping_rule_hash", &request.stopping_rule_hash)?;
    Ok(())
}

async fn create_experiment_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateExperimentPlanRequest>,
) -> Result<(StatusCode, Json<ExperimentPlan>), ApiError> {
    const OPERATION: &str = "create_experiment_plan_v3";
    validate_experiment_plan_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/experiment-plans");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let manifest_ids = [
        request.code_manifest_id,
        request.dataset_manifest_id,
        request.environment_manifest_id,
    ];
    validate_distinct_ids("experiment_manifest_ids", &manifest_ids, false)?;
    let plan = ExperimentPlan {
        experiment_plan_id: request.experiment_plan_id,
        paper_project_id: paper_id,
        protocol_snapshot_hash: request.protocol_snapshot_hash.clone(),
        code_manifest_id: request.code_manifest_id,
        dataset_manifest_id: request.dataset_manifest_id,
        environment_manifest_id: request.environment_manifest_id,
        seed_policy_hash: request.seed_policy_hash.clone(),
        stopping_rule_hash: request.stopping_rule_hash.clone(),
        version: 1,
        created_at: Utc::now(),
    };
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::ExperimentPlan)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if manifest_ids.iter().any(|id| {
            memory
                .collaboration
                .artifact_manifests
                .get(id)
                .is_none_or(|manifest| manifest.paper_project_id != paper_id)
        }) {
            return Err(ApiError::bad_request(
                "cross_paper_or_missing_manifest",
                "experiment plan manifests must all belong to this paper",
            ));
        }
        if memory
            .collaboration
            .experiment_plans
            .insert(plan.experiment_plan_id, plan.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "experiment_plan_exists",
                "experiment_plan_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.experiment_plan.created.v1",
            paper_id,
            plan.experiment_plan_id,
            1,
            json!({"experiment_plan_id":plan.experiment_plan_id,"protocol_snapshot_hash":plan.protocol_snapshot_hash}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &plan,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(plan)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::ExperimentPlan)?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_artifact_manifests",
        "manifest_id",
        paper_id,
        &manifest_ids,
        "experiment_manifest_ids",
    )
    .await?;
    sqlx::query(
        "insert into hepta_experiment_plans (
            experiment_plan_id,paper_project_id,protocol_snapshot_hash,code_manifest_id,
            dataset_manifest_id,environment_manifest_id,seed_policy_hash,stopping_rule_hash,
            version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,1,$9::jsonb,$10)",
    )
    .bind(plan.experiment_plan_id)
    .bind(paper_id)
    .bind(&plan.protocol_snapshot_hash)
    .bind(plan.code_manifest_id)
    .bind(plan.dataset_manifest_id)
    .bind(plan.environment_manifest_id)
    .bind(&plan.seed_policy_hash)
    .bind(&plan.stopping_rule_hash)
    .bind(
        serde_json::to_value(&plan)
            .map_err(|error| ApiError::internal(format!("encode experiment plan: {error}")))?,
    )
    .bind(plan.created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("experiment_plan_exists", "experiment plan already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(&mut tx, OPERATION, &request.idempotency_key,
        "hepta.paper_raid.experiment_plan.created.v1", paper_id, plan.experiment_plan_id, 1,
        json!({"experiment_plan_id":plan.experiment_plan_id,"protocol_snapshot_hash":plan.protocol_snapshot_hash})).await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(plan.experiment_plan_id),
        StatusCode::CREATED,
        &plan,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(plan)))
}

fn validate_run_request(request: &CreateRunRecordRequest) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("parameters_hash", &request.parameters_hash)?;
    if let Some(hash) = &request.metrics_hash {
        validate_digest_v2("metrics_hash", hash)?;
    }
    if let Some(hash) = &request.failure_hash {
        validate_digest_v2("failure_hash", hash)?;
    }
    match request.status {
        RunStatus::Succeeded if request.outputs_manifest_id.is_some()
            && request.metrics_hash.is_some() && request.failure_hash.is_none() => Ok(()),
        RunStatus::Failed | RunStatus::Cancelled if request.failure_hash.is_some() => Ok(()),
        _ => Err(ApiError::bad_request(
            "invalid_run_outcome",
            "successful runs require outputs+metrics and no failure; failed/cancelled runs require failure_hash",
        )),
    }
}

async fn create_run_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateRunRecordRequest>,
) -> Result<(StatusCode, Json<RunRecord>), ApiError> {
    const OPERATION: &str = "create_run_record_v3";
    validate_run_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/run-records");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let mut manifest_ids = vec![request.logs_manifest_id];
    if let Some(id) = request.outputs_manifest_id {
        manifest_ids.push(id);
    }
    validate_distinct_ids("run_manifest_ids", &manifest_ids, false)?;
    let record = RunRecord {
        run_record_id: request.run_record_id,
        paper_project_id: paper_id,
        experiment_plan_id: request.experiment_plan_id,
        status: request.status.clone(),
        seed: request.seed,
        parameters_hash: request.parameters_hash.clone(),
        logs_manifest_id: request.logs_manifest_id,
        outputs_manifest_id: request.outputs_manifest_id,
        metrics_hash: request.metrics_hash.clone(),
        failure_hash: request.failure_hash.clone(),
        version: 1,
        created_at: Utc::now(),
    };
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Run)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if memory
            .collaboration
            .experiment_plans
            .get(&request.experiment_plan_id)
            .is_none_or(|plan| plan.paper_project_id != paper_id)
            || manifest_ids.iter().any(|id| {
                memory
                    .collaboration
                    .artifact_manifests
                    .get(id)
                    .is_none_or(|manifest| manifest.paper_project_id != paper_id)
            })
        {
            return Err(ApiError::bad_request(
                "cross_paper_or_missing_run_input",
                "run plan and manifests must all belong to this paper",
            ));
        }
        if memory
            .collaboration
            .runs
            .insert(record.run_record_id, record.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "run_record_exists",
                "run_record_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.run_record.created.v1",
            paper_id,
            record.run_record_id,
            1,
            json!({"run_record_id":record.run_record_id,"status":record.status,"failure_retained":record.failure_hash.is_some()}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &record,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(record)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Run)?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_experiment_plans",
        "experiment_plan_id",
        paper_id,
        &[request.experiment_plan_id],
        "experiment_plan_id",
    )
    .await?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_artifact_manifests",
        "manifest_id",
        paper_id,
        &manifest_ids,
        "run_manifest_ids",
    )
    .await?;
    sqlx::query(
        "insert into hepta_run_records (
            run_record_id,paper_project_id,experiment_plan_id,status,seed,parameters_hash,
            logs_manifest_id,outputs_manifest_id,metrics_hash,failure_hash,version,record_json,created_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11::jsonb,$12)")
        .bind(record.run_record_id).bind(paper_id).bind(record.experiment_plan_id).bind(record.status.as_str())
        .bind(record.seed).bind(&record.parameters_hash).bind(record.logs_manifest_id)
        .bind(record.outputs_manifest_id).bind(&record.metrics_hash).bind(&record.failure_hash)
        .bind(serde_json::to_value(&record).map_err(|error| ApiError::internal(format!("encode run record: {error}")))?)
        .bind(record.created_at).execute(&mut *tx).await.map_err(|error| match &error {
            sqlx::Error::Database(database) if database.is_unique_violation() =>
                ApiError::conflict("run_record_exists", "run record already exists"),
            _ => ApiError::database(error),
        })?;
    insert_room_event_postgres(&mut tx, OPERATION, &request.idempotency_key,
        "hepta.paper_raid.run_record.created.v1", paper_id, record.run_record_id, 1,
        json!({"run_record_id":record.run_record_id,"status":record.status,"failure_retained":record.failure_hash.is_some()})).await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(record.run_record_id),
        StatusCode::CREATED,
        &record,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn create_figure_lineage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateFigureLineageRequest>,
) -> Result<(StatusCode, Json<FigureLineage>), ApiError> {
    const OPERATION: &str = "create_figure_lineage_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_section_key(&request.figure_key)?;
    validate_digest_v2("transform_hash", &request.transform_hash)?;
    validate_distinct_ids("run_record_ids", &request.run_record_ids, false)?;
    let path = format!("/v2/hepta/papers/{paper_id}/figure-lineage");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let record = FigureLineage {
        figure_lineage_id: request.figure_lineage_id,
        paper_project_id: paper_id,
        figure_key: request.figure_key.clone(),
        figure_manifest_id: request.figure_manifest_id,
        run_record_ids: request.run_record_ids.clone(),
        transform_hash: request.transform_hash.clone(),
        version: 1,
        created_at: Utc::now(),
    };
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Figure)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if memory
            .collaboration
            .artifact_manifests
            .get(&request.figure_manifest_id)
            .is_none_or(|manifest| manifest.paper_project_id != paper_id)
            || request.run_record_ids.iter().any(|id| {
                memory
                    .collaboration
                    .runs
                    .get(id)
                    .is_none_or(|run| run.paper_project_id != paper_id)
            })
        {
            return Err(ApiError::bad_request(
                "cross_paper_or_missing_figure_input",
                "figure manifest and runs must all belong to this paper",
            ));
        }
        if memory.collaboration.figures.values().any(|figure| {
            figure.paper_project_id == paper_id && figure.figure_key == record.figure_key
        }) || memory
            .collaboration
            .figures
            .insert(record.figure_lineage_id, record.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "figure_lineage_exists",
                "figure id or paper-scoped figure_key already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.figure_lineage.created.v1",
            paper_id,
            record.figure_lineage_id,
            1,
            json!({"figure_lineage_id":record.figure_lineage_id,"run_record_ids":record.run_record_ids}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &record,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(record)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Figure)?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_artifact_manifests",
        "manifest_id",
        paper_id,
        &[request.figure_manifest_id],
        "figure_manifest_id",
    )
    .await?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_run_records",
        "run_record_id",
        paper_id,
        &request.run_record_ids,
        "run_record_ids",
    )
    .await?;
    sqlx::query(
        "insert into hepta_figure_lineage (
        figure_lineage_id,paper_project_id,figure_key,figure_manifest_id,run_record_ids,
        transform_hash,version,record_json,created_at) values ($1,$2,$3,$4,$5,$6,1,$7::jsonb,$8)",
    )
    .bind(record.figure_lineage_id)
    .bind(paper_id)
    .bind(&record.figure_key)
    .bind(record.figure_manifest_id)
    .bind(&record.run_record_ids)
    .bind(&record.transform_hash)
    .bind(
        serde_json::to_value(&record)
            .map_err(|error| ApiError::internal(format!("encode figure lineage: {error}")))?,
    )
    .bind(record.created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("figure_lineage_exists", "figure lineage already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(&mut tx,OPERATION,&request.idempotency_key,
        "hepta.paper_raid.figure_lineage.created.v1",paper_id,record.figure_lineage_id,1,
        json!({"figure_lineage_id":record.figure_lineage_id,"run_record_ids":record.run_record_ids})).await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(record.figure_lineage_id),
        StatusCode::CREATED,
        &record,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn create_claim_record(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateClaimRecordRequest>,
) -> Result<(StatusCode, Json<ClaimRecord>), ApiError> {
    const OPERATION: &str = "create_claim_record_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_section_key(&request.claim_key)?;
    validate_digest_v2("statement_hash", &request.statement_hash)?;
    validate_distinct_ids("evidence_card_ids", &request.evidence_card_ids, true)?;
    validate_distinct_ids("run_record_ids", &request.run_record_ids, true)?;
    validate_distinct_ids("figure_lineage_ids", &request.figure_lineage_ids, true)?;
    if request.claim_kind.requires_lineage()
        && request.evidence_card_ids.is_empty()
        && request.run_record_ids.is_empty()
        && request.figure_lineage_ids.is_empty()
    {
        return Err(ApiError::bad_request(
            "claim_lineage_required",
            "main, numeric, and figure claims require evidence, run, or figure lineage",
        ));
    }
    let path = format!("/v2/hepta/papers/{paper_id}/claims");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();
    let record = ClaimRecord {
        claim_id: request.claim_id,
        paper_project_id: paper_id,
        claim_key: request.claim_key.clone(),
        claim_kind: request.claim_kind.clone(),
        statement_hash: request.statement_hash.clone(),
        evidence_card_ids: request.evidence_card_ids.clone(),
        run_record_ids: request.run_record_ids.clone(),
        figure_lineage_ids: request.figure_lineage_ids.clone(),
        status: "proposed".to_string(),
        version: 1,
        created_at: now,
        updated_at: now,
    };
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Claim)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if request.evidence_card_ids.iter().any(|id| {
            memory
                .collaboration
                .evidence_cards
                .get(id)
                .is_none_or(|item| item.paper_project_id != paper_id)
        }) || request.run_record_ids.iter().any(|id| {
            memory
                .collaboration
                .runs
                .get(id)
                .is_none_or(|item| item.paper_project_id != paper_id)
        }) || request.figure_lineage_ids.iter().any(|id| {
            memory
                .collaboration
                .figures
                .get(id)
                .is_none_or(|item| item.paper_project_id != paper_id)
        }) {
            return Err(ApiError::bad_request(
                "cross_paper_or_missing_claim_lineage",
                "claim lineage references must all belong to this paper",
            ));
        }
        if memory
            .collaboration
            .claims
            .values()
            .any(|claim| claim.paper_project_id == paper_id && claim.claim_key == record.claim_key)
            || memory
                .collaboration
                .claims
                .insert(record.claim_id, record.clone())
                .is_some()
        {
            return Err(ApiError::conflict(
                "claim_exists",
                "claim id or paper-scoped claim_key already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.claim.created.v1",
            paper_id,
            record.claim_id,
            1,
            json!({"claim_id":record.claim_id,"claim_kind":record.claim_kind,"status":record.status}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &record,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(record)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Claim)?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_evidence_cards",
        "evidence_card_id",
        paper_id,
        &request.evidence_card_ids,
        "evidence_card_ids",
    )
    .await?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_run_records",
        "run_record_id",
        paper_id,
        &request.run_record_ids,
        "run_record_ids",
    )
    .await?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_figure_lineage",
        "figure_lineage_id",
        paper_id,
        &request.figure_lineage_ids,
        "figure_lineage_ids",
    )
    .await?;
    sqlx::query(
        "insert into hepta_claim_records (
        claim_id,paper_project_id,claim_key,claim_kind,statement_hash,evidence_card_ids,
        run_record_ids,figure_lineage_ids,status,version,record_json,created_at,updated_at
      ) values ($1,$2,$3,$4,$5,$6,$7,$8,'proposed',1,$9::jsonb,$10,$10)",
    )
    .bind(record.claim_id)
    .bind(paper_id)
    .bind(&record.claim_key)
    .bind(record.claim_kind.as_str())
    .bind(&record.statement_hash)
    .bind(&record.evidence_card_ids)
    .bind(&record.run_record_ids)
    .bind(&record.figure_lineage_ids)
    .bind(
        serde_json::to_value(&record)
            .map_err(|error| ApiError::internal(format!("encode claim: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("claim_exists", "claim already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.claim.created.v1",
        paper_id,
        record.claim_id,
        1,
        json!({"claim_id":record.claim_id,"claim_kind":record.claim_kind,"status":record.status}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(record.claim_id),
        StatusCode::CREATED,
        &record,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(record)))
}

fn validate_lease_request(request: &AcquireSectionLeaseRequest) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_section_key(&request.section_key)?;
    if !(60..=3_600).contains(&request.ttl_seconds) {
        return Err(ApiError::bad_request(
            "invalid_lease_ttl",
            "section lease ttl_seconds must be between 60 and 3600",
        ));
    }
    Ok(())
}

fn active_team_binding_memory(
    memory: &PaperRaidMemory,
    team: &ResearchTeam,
    player_id: Uuid,
    binding_id: Uuid,
) -> Result<AgentBinding, ApiError> {
    let member = team
        .members
        .iter()
        .find(|member| member.player_id == player_id && member.binding_id == binding_id)
        .ok_or_else(|| {
            ApiError::forbidden(
                "binding_not_on_team",
                "Agent binding is not the asserted player's binding on this team",
            )
        })?;
    let binding = memory.bindings.get(&binding_id).cloned().ok_or_else(|| {
        ApiError::forbidden("agent_binding_not_found", "Agent binding does not exist")
    })?;
    if binding.status != AgentBindingStatus::Active
        || binding.player_id != player_id
        || binding.agent_id != member.agent_id
    {
        return Err(ApiError::forbidden(
            "agent_binding_not_active",
            "Agent binding must be current, active, and match the frozen team member",
        ));
    }
    Ok(binding)
}

async fn active_team_binding_postgres(
    tx: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    player_id: Uuid,
    binding_id: Uuid,
) -> Result<AgentBinding, ApiError> {
    let row = sqlx::query(
        "select b.record_json from hepta_research_team_members m
         join hepta_agent_bindings b
           on b.binding_id=m.binding_id and b.player_id=m.player_id
         where m.team_id=$1 and m.player_id=$2 and m.binding_id=$3 and b.status='active'
         for share of m,b",
    )
    .bind(team_id)
    .bind(player_id)
    .bind(binding_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "agent_binding_not_active",
            "Agent binding must be current, active, and match the frozen team member",
        )
    })?;
    decode_record(row.get("record_json"), "Agent binding")
}

async fn acquire_section_lease(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<AcquireSectionLeaseRequest>,
) -> Result<(StatusCode, Json<SectionLease>), ApiError> {
    const OPERATION: &str = "acquire_section_lease_v3";
    validate_lease_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/section-leases");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();
    let expires_at =
        now + chrono::Duration::seconds(i64::try_from(request.ttl_seconds).expect("ttl fits i64"));
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        active_team_binding_memory(
            &memory,
            &team,
            assertion.player_id,
            request.holder_binding_id,
        )?;
        let base_revision_id = paper.current_revision_id.ok_or_else(|| {
            ApiError::conflict(
                "paper_baseline_required",
                "a whole-paper revision must exist before section collaboration begins",
            )
        })?;
        if memory
            .revisions
            .get(&base_revision_id)
            .is_none_or(|revision| revision.paper_project_id != paper_id)
        {
            return Err(ApiError::internal(
                "paper current revision is missing or belongs to another paper",
            ));
        }
        let head_key = (paper_id, request.section_key.clone());
        let head = memory
            .collaboration
            .section_heads
            .entry(head_key)
            .or_insert_with(|| SectionHead {
                paper_project_id: paper_id,
                section_key: request.section_key.clone(),
                base_paper_revision_id: base_revision_id,
                current_head_revision_id: base_revision_id,
                fencing_token: 0,
                version: 1,
                updated_at: now,
            });
        if head.base_paper_revision_id != base_revision_id {
            return Err(ApiError::conflict(
                "paper_baseline_changed",
                "section head belongs to an older whole-paper revision and requires explicit rebase",
            ));
        }
        let active_id = memory
            .collaboration
            .leases
            .values()
            .find(|lease| {
                lease.paper_project_id == paper_id
                    && lease.section_key == request.section_key
                    && lease.status == SectionLeaseStatus::Active
            })
            .map(|lease| lease.lease_id);
        if let Some(active_id) = active_id {
            let active = memory
                .collaboration
                .leases
                .get_mut(&active_id)
                .expect("selected lease exists");
            if active.expires_at > now {
                return Err(ApiError::conflict(
                    "section_lease_active",
                    "section already has an unexpired active lease",
                ));
            }
            active.status = SectionLeaseStatus::Expired;
            active.version += 1;
            active.updated_at = now;
        }
        let head = memory
            .collaboration
            .section_heads
            .get_mut(&(paper_id, request.section_key.clone()))
            .expect("section head exists");
        if request.expected_previous_fencing_token != head.fencing_token {
            return Err(ApiError::conflict(
                "stale_fencing_token",
                "expected_previous_fencing_token does not match the section head",
            ));
        }
        head.fencing_token = head.fencing_token.checked_add(1).ok_or_else(|| {
            ApiError::conflict("fencing_token_exhausted", "section fencing token overflow")
        })?;
        head.version += 1;
        head.updated_at = now;
        let lease = SectionLease {
            lease_id: request.lease_id,
            paper_project_id: paper_id,
            section_key: request.section_key.clone(),
            holder_player_id: assertion.player_id,
            holder_binding_id: request.holder_binding_id,
            fencing_token: head.fencing_token,
            status: SectionLeaseStatus::Active,
            version: 1,
            acquired_at: now,
            expires_at,
            updated_at: now,
        };
        if memory
            .collaboration
            .leases
            .insert(lease.lease_id, lease.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "section_lease_exists",
                "lease_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.section_lease.acquired.v1",
            paper_id,
            lease.lease_id,
            1,
            json!({"lease_id":lease.lease_id,"section_key":lease.section_key,"fencing_token":lease.fencing_token}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &lease,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(lease)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("paper-section:{paper_id}:{}", request.section_key))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
    active_team_binding_postgres(
        &mut tx,
        team.team_id,
        assertion.player_id,
        request.holder_binding_id,
    )
    .await?;
    let base_revision_id = paper.current_revision_id.ok_or_else(|| {
        ApiError::conflict(
            "paper_baseline_required",
            "a whole-paper revision must exist before section collaboration begins",
        )
    })?;
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_paper_revisions",
        "revision_id",
        paper_id,
        &[base_revision_id],
        "base_paper_revision_id",
    )
    .await?;
    sqlx::query(
        "insert into hepta_section_heads (
            paper_project_id,section_key,base_paper_revision_id,current_head_revision_id,
            fencing_token,version,updated_at
         ) values ($1,$2,$3,$3,0,1,$4)
         on conflict (paper_project_id,section_key) do nothing",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .bind(base_revision_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let head_row = sqlx::query(
        "select base_paper_revision_id,current_head_revision_id,fencing_token,version
         from hepta_section_heads where paper_project_id=$1 and section_key=$2 for update",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if head_row.get::<Uuid, _>("base_paper_revision_id") != base_revision_id {
        return Err(ApiError::conflict(
            "paper_baseline_changed",
            "section head belongs to an older whole-paper revision and requires explicit rebase",
        ));
    }
    let previous_token_i64: i64 = head_row.get("fencing_token");
    let previous_token = u64::try_from(previous_token_i64)
        .map_err(|_| ApiError::internal("negative section fencing token"))?;
    if previous_token != request.expected_previous_fencing_token {
        return Err(ApiError::conflict(
            "stale_fencing_token",
            "expected_previous_fencing_token does not match the section head",
        ));
    }
    if let Some(row) = sqlx::query(
        "select record_json from hepta_section_leases
         where paper_project_id=$1 and section_key=$2 and status='active' for update",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    {
        let mut active: SectionLease = decode_record(row.get("record_json"), "section lease")?;
        if active.expires_at > now {
            return Err(ApiError::conflict(
                "section_lease_active",
                "section already has an unexpired active lease",
            ));
        }
        active.status = SectionLeaseStatus::Expired;
        active.version += 1;
        active.updated_at = now;
        sqlx::query(
            "update hepta_section_leases set status='expired',version=$1,
             record_json=$2::jsonb,updated_at=$3 where lease_id=$4 and status='active'",
        )
        .bind(
            i64::try_from(active.version)
                .map_err(|_| ApiError::internal("lease version overflow"))?,
        )
        .bind(serde_json::to_value(&active).map_err(|error| {
            ApiError::internal(format!("encode expired section lease: {error}"))
        })?)
        .bind(now)
        .bind(active.lease_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    }
    let fencing_token = previous_token.checked_add(1).ok_or_else(|| {
        ApiError::conflict("fencing_token_exhausted", "section fencing token overflow")
    })?;
    let lease = SectionLease {
        lease_id: request.lease_id,
        paper_project_id: paper_id,
        section_key: request.section_key.clone(),
        holder_player_id: assertion.player_id,
        holder_binding_id: request.holder_binding_id,
        fencing_token,
        status: SectionLeaseStatus::Active,
        version: 1,
        acquired_at: now,
        expires_at,
        updated_at: now,
    };
    sqlx::query(
        "insert into hepta_section_leases (
            lease_id,paper_project_id,section_key,holder_player_id,holder_binding_id,
            fencing_token,status,version,record_json,acquired_at,expires_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,'active',1,$7::jsonb,$8,$9,$8)",
    )
    .bind(lease.lease_id)
    .bind(paper_id)
    .bind(&lease.section_key)
    .bind(lease.holder_player_id)
    .bind(lease.holder_binding_id)
    .bind(
        i64::try_from(lease.fencing_token)
            .map_err(|_| ApiError::internal("fencing token overflow"))?,
    )
    .bind(
        serde_json::to_value(&lease)
            .map_err(|error| ApiError::internal(format!("encode section lease: {error}")))?,
    )
    .bind(now)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("section_lease_exists", "section lease already exists")
        }
        _ => ApiError::database(error),
    })?;
    sqlx::query(
        "update hepta_section_heads
         set fencing_token=$1,version=version+1,updated_at=$2
         where paper_project_id=$3 and section_key=$4 and fencing_token=$5",
    )
    .bind(i64::try_from(fencing_token).map_err(|_| ApiError::internal("fencing token overflow"))?)
    .bind(now)
    .bind(paper_id)
    .bind(&request.section_key)
    .bind(previous_token_i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.section_lease.acquired.v1",
        paper_id,
        lease.lease_id,
        1,
        json!({"lease_id":lease.lease_id,"section_key":lease.section_key,"fencing_token":lease.fencing_token}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(lease.lease_id),
        StatusCode::CREATED,
        &lease,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(lease)))
}

fn validate_agent_proposal_request(request: &CreateAgentProposalRequest) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_section_key(&request.section_key)?;
    validate_contract_text_api("agent_id", &request.agent_id)?;
    validate_contract_text_api("agent_key_id", &request.agent_key_id)?;
    validate_digest_v2("payload_hash", &request.payload_hash)?;
    signed_time(request.signed_at_unix)?;
    Ok(())
}

fn verify_agent_proposal(
    paper_id: Uuid,
    request: &CreateAgentProposalRequest,
    artifact_manifest_hash: &str,
    registration: &crate::AgentRegistration,
) -> Result<String, ApiError> {
    if registration.agent_id != request.agent_id {
        return Err(ApiError::forbidden(
            "agent_registration_mismatch",
            "proposal agent_id is not currently registered",
        ));
    }
    let public_key = canonical_public_key(&registration.public_key)?;
    let public_key_bytes = BASE64
        .decode(&public_key)
        .map_err(|_| ApiError::internal("canonical Agent public key decode failed"))?;
    let expected_key_id = crate::paper_raid_contracts::sha256_digest(&public_key_bytes);
    if request.agent_key_id != expected_key_id {
        return Err(ApiError::forbidden(
            "agent_key_not_current",
            "agent_key_id does not match the currently registered Agent key",
        ));
    }
    let signing = AgentProposalSigningV1 {
        schema: AGENT_PROPOSAL_V1.to_string(),
        proposal_id: request.proposal_id,
        paper_project_id: paper_id,
        work_item_id: request.work_item_id,
        section_key: request.section_key.clone(),
        parent_revision_id: request.parent_revision_id,
        proposal_kind: request.proposal_kind.as_str().to_string(),
        payload_hash: request.payload_hash.clone(),
        artifact_manifest_hash: artifact_manifest_hash.to_string(),
        agent_id: request.agent_id.clone(),
        binding_id: request.binding_id,
        agent_key_id: request.agent_key_id.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let signing_bytes = agent_proposal_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_agent_proposal", message))?;
    let verifying_key = crate::decode_verifying_key(&public_key)?;
    verifying_key
        .verify(&signing_bytes, &decode_signature(&request.signature)?)
        .map_err(|_| {
            ApiError::forbidden(
                "agent_proposal_signature_failed",
                "Agent proposal signature verification failed",
            )
        })?;
    Ok(public_key)
}

fn validate_agent_scope_memory(
    memory: &PaperRaidMemory,
    paper: &PaperProject,
    team: &ResearchTeam,
    request: &CreateAgentProposalRequest,
) -> Result<(WorkItem, ArtifactManifest, AgentBinding), ApiError> {
    let binding = memory
        .bindings
        .get(&request.binding_id)
        .cloned()
        .filter(|binding| {
            binding.status == AgentBindingStatus::Active && binding.agent_id == request.agent_id
        })
        .ok_or_else(|| {
            ApiError::forbidden(
                "agent_binding_not_active",
                "proposal Agent binding is missing, revoked, or belongs to another Agent",
            )
        })?;
    if !team.members.iter().any(|member| {
        member.player_id == binding.player_id
            && member.binding_id == binding.binding_id
            && member.agent_id == binding.agent_id
    }) {
        return Err(ApiError::forbidden(
            "agent_binding_not_on_team",
            "proposal Agent binding is not on the paper roster",
        ));
    }
    let work = memory
        .work_items
        .get(&request.work_item_id)
        .cloned()
        .filter(|work| work.paper_project_id == paper.paper_project_id)
        .ok_or_else(|| {
            ApiError::bad_request(
                "cross_paper_or_missing_work_item",
                "proposal work item is missing or belongs to another paper",
            )
        })?;
    if work.assigned_binding_id != Some(request.binding_id)
        || work.assigned_player_id != Some(binding.player_id)
    {
        return Err(ApiError::forbidden(
            "work_item_not_assigned_to_agent",
            "proposal work item is not assigned to this player/Agent binding",
        ));
    }
    let manifest = memory
        .collaboration
        .artifact_manifests
        .get(&request.artifact_manifest_id)
        .cloned()
        .filter(|manifest| manifest.paper_project_id == paper.paper_project_id)
        .ok_or_else(|| {
            ApiError::bad_request(
                "cross_paper_or_missing_manifest",
                "proposal artifact manifest is missing or belongs to another paper",
            )
        })?;
    let head = memory
        .collaboration
        .section_heads
        .get(&(paper.paper_project_id, request.section_key.clone()))
        .ok_or_else(|| {
            ApiError::conflict(
                "section_head_not_initialized",
                "acquire a section lease before submitting an Agent proposal",
            )
        })?;
    if head.current_head_revision_id != request.parent_revision_id
        || request.parent_revision_id == request.proposal_id
    {
        return Err(ApiError::conflict(
            "stale_section_parent",
            "proposal parent must be the current same-section head and may not self-reference",
        ));
    }
    Ok((work, manifest, binding))
}

async fn create_agent_proposal(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateAgentProposalRequest>,
) -> Result<(StatusCode, Json<AgentProposal>), ApiError> {
    const OPERATION: &str = "create_agent_proposal_v3";
    validate_agent_proposal_request(&request)?;
    let request_hash = request_hash(&request)?;
    let registration = state
        .inspect(|league| {
            league
                .agents
                .get(&request.agent_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::forbidden("agent_not_registered", "Agent is not registered")
                })
        })
        .await?;
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        let (_, manifest, _) = validate_agent_scope_memory(&memory, &paper, &team, &request)?;
        let public_key =
            verify_agent_proposal(paper_id, &request, &manifest.manifest_hash, &registration)?;
        let proposal = AgentProposal {
            proposal_id: request.proposal_id,
            paper_project_id: paper_id,
            work_item_id: request.work_item_id,
            section_key: request.section_key.clone(),
            parent_revision_id: request.parent_revision_id,
            proposal_kind: request.proposal_kind.clone(),
            payload_hash: request.payload_hash.clone(),
            artifact_manifest_id: request.artifact_manifest_id,
            artifact_manifest_hash: manifest.manifest_hash,
            agent_id: request.agent_id.clone(),
            binding_id: request.binding_id,
            agent_key_id: request.agent_key_id.clone(),
            agent_public_key: public_key,
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            status: AgentProposalStatus::Submitted,
            version: 1,
            updated_at: Utc::now(),
        };
        if memory
            .collaboration
            .proposals
            .insert(proposal.proposal_id, proposal.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "agent_proposal_exists",
                "proposal_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.agent_proposal.submitted.v1",
            paper_id,
            proposal.proposal_id,
            1,
            json!({"proposal_id":proposal.proposal_id,"work_item_id":proposal.work_item_id,"section_key":proposal.section_key}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &proposal,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(proposal)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let league_row = sqlx::query(
        "select state_json from hepta_league_state where state_key='primary' for share",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let league: crate::LeagueState = decode_record(league_row.get("state_json"), "league state")?;
    let registration = league
        .agents
        .get(&request.agent_id)
        .ok_or_else(|| ApiError::forbidden("agent_not_registered", "Agent is not registered"))?;
    let paper_row = sqlx::query(
        "select record_json from hepta_paper_projects where paper_project_id=$1 for share",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
    let scope_row = sqlx::query(
        "select w.record_json as work_json,m.record_json as manifest_json,b.record_json as binding_json
         from hepta_paper_work_items w
         join hepta_artifact_manifests m
           on m.manifest_id=$2 and m.paper_project_id=w.paper_project_id
         join hepta_agent_bindings b on b.binding_id=$3 and b.agent_id=$4 and b.status='active'
         join hepta_research_team_members tm
           on tm.team_id=$5 and tm.binding_id=b.binding_id and tm.player_id=b.player_id
         where w.work_item_id=$1 and w.paper_project_id=$6
           and w.assigned_binding_id=b.binding_id and w.assigned_player_id=b.player_id
         for share of w,m,b,tm",
    )
    .bind(request.work_item_id)
    .bind(request.artifact_manifest_id)
    .bind(request.binding_id)
    .bind(&request.agent_id)
    .bind(paper.team_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "agent_proposal_scope_mismatch",
            "work item, artifact, binding, and team assignment must share this paper scope",
        )
    })?;
    let manifest: ArtifactManifest =
        decode_record(scope_row.get("manifest_json"), "artifact manifest")?;
    let head_row = sqlx::query(
        "select current_head_revision_id from hepta_section_heads
         where paper_project_id=$1 and section_key=$2 for share",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "section_head_not_initialized",
            "acquire a section lease before submitting an Agent proposal",
        )
    })?;
    if head_row.get::<Uuid, _>("current_head_revision_id") != request.parent_revision_id
        || request.parent_revision_id == request.proposal_id
    {
        return Err(ApiError::conflict(
            "stale_section_parent",
            "proposal parent must be the current same-section head and may not self-reference",
        ));
    }
    let public_key =
        verify_agent_proposal(paper_id, &request, &manifest.manifest_hash, registration)?;
    let proposal = AgentProposal {
        proposal_id: request.proposal_id,
        paper_project_id: paper_id,
        work_item_id: request.work_item_id,
        section_key: request.section_key.clone(),
        parent_revision_id: request.parent_revision_id,
        proposal_kind: request.proposal_kind.clone(),
        payload_hash: request.payload_hash.clone(),
        artifact_manifest_id: request.artifact_manifest_id,
        artifact_manifest_hash: manifest.manifest_hash,
        agent_id: request.agent_id.clone(),
        binding_id: request.binding_id,
        agent_key_id: request.agent_key_id.clone(),
        agent_public_key: public_key,
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
        status: AgentProposalStatus::Submitted,
        version: 1,
        updated_at: Utc::now(),
    };
    sqlx::query(
        "insert into hepta_agent_proposals (
            proposal_id,paper_project_id,work_item_id,section_key,parent_revision_id,
            proposal_kind,payload_hash,artifact_manifest_id,agent_id,binding_id,
            agent_key_id,agent_public_key,signature,status,version,record_json,signed_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'submitted',1,$14::jsonb,$15,$16)",
    )
    .bind(proposal.proposal_id)
    .bind(paper_id)
    .bind(proposal.work_item_id)
    .bind(&proposal.section_key)
    .bind(proposal.parent_revision_id)
    .bind(proposal.proposal_kind.as_str())
    .bind(&proposal.payload_hash)
    .bind(proposal.artifact_manifest_id)
    .bind(&proposal.agent_id)
    .bind(proposal.binding_id)
    .bind(&proposal.agent_key_id)
    .bind(&proposal.agent_public_key)
    .bind(&proposal.signature)
    .bind(
        serde_json::to_value(&proposal)
            .map_err(|error| ApiError::internal(format!("encode Agent proposal: {error}")))?,
    )
    .bind(signed_time(proposal.signed_at_unix)?)
    .bind(proposal.updated_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("agent_proposal_exists", "Agent proposal already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.agent_proposal.submitted.v1",
        paper_id,
        proposal.proposal_id,
        1,
        json!({"proposal_id":proposal.proposal_id,"work_item_id":proposal.work_item_id,"section_key":proposal.section_key}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(proposal.proposal_id),
        StatusCode::CREATED,
        &proposal,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(proposal)))
}

fn verify_human_decision(
    paper_id: Uuid,
    player_id: Uuid,
    request: &CreateHumanDecisionRequest,
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("reason_hash", &request.reason_hash)?;
    signed_time(request.signed_at_unix)?;
    let signing = HumanDecisionSigningV1 {
        schema: HUMAN_DECISION_V1.to_string(),
        decision_id: request.decision_id,
        paper_project_id: paper_id,
        proposal_id: request.proposal_id,
        player_id,
        decision: request.decision.as_str().to_string(),
        reason_hash: request.reason_hash.clone(),
        expected_proposal_version: request.expected_proposal_version,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let bytes = human_decision_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_human_decision", message))?;
    verifying_key
        .verify(&bytes, &decode_signature(&request.signature)?)
        .map_err(|_| {
            ApiError::forbidden(
                "human_decision_signature_failed",
                "human decision signature verification failed",
            )
        })
}

async fn create_human_decision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateHumanDecisionRequest>,
) -> Result<(StatusCode, Json<HumanDecision>), ApiError> {
    const OPERATION: &str = "create_human_decision_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    let path = format!("/v2/hepta/papers/{paper_id}/human-decisions");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let verifying_key = active_human_key_memory(
            &memory,
            assertion.player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
            request.signed_at_unix,
        )?;
        verify_human_decision(paper_id, assertion.player_id, &request, &verifying_key)?;
        if memory.collaboration.decisions.values().any(|decision| {
            decision.proposal_id == request.proposal_id && decision.player_id == assertion.player_id
        }) {
            return Err(ApiError::conflict(
                "human_decision_exists",
                "player already decided this proposal",
            ));
        }
        let proposal = memory
            .collaboration
            .proposals
            .get_mut(&request.proposal_id)
            .filter(|proposal| proposal.paper_project_id == paper_id)
            .ok_or_else(|| {
                ApiError::bad_request(
                    "cross_paper_or_missing_proposal",
                    "proposal is missing or belongs to another paper",
                )
            })?;
        if proposal.version != request.expected_proposal_version {
            return Err(version_conflict(
                "Agent proposal",
                request.expected_proposal_version,
                proposal.version,
            ));
        }
        if proposal.status != AgentProposalStatus::Submitted {
            return Err(ApiError::conflict(
                "proposal_already_decided",
                "Agent proposal is no longer submitted",
            ));
        }
        proposal.status = request.decision.proposal_status();
        proposal.version += 1;
        proposal.updated_at = Utc::now();
        let proposal_version = proposal.version;
        let decision = HumanDecision {
            decision_id: request.decision_id,
            paper_project_id: paper_id,
            proposal_id: request.proposal_id,
            player_id: assertion.player_id,
            decision: request.decision.clone(),
            reason_hash: request.reason_hash.clone(),
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            version: 1,
        };
        if memory
            .collaboration
            .decisions
            .insert(decision.decision_id, decision.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "human_decision_exists",
                "decision_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.human_decision.created.v1",
            paper_id,
            decision.decision_id,
            1,
            json!({"decision_id":decision.decision_id,"proposal_id":decision.proposal_id,"decision":decision.decision,"proposal_version":proposal_version}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &decision,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(decision)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
    let verifying_key = active_human_key_postgres(
        &mut tx,
        assertion.player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
        request.signed_at_unix,
    )
    .await?;
    verify_human_decision(paper_id, assertion.player_id, &request, &verifying_key)?;
    let proposal_row = sqlx::query(
        "select record_json from hepta_agent_proposals
         where proposal_id=$1 and paper_project_id=$2 for update",
    )
    .bind(request.proposal_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::bad_request(
            "cross_paper_or_missing_proposal",
            "proposal is missing or belongs to another paper",
        )
    })?;
    let mut proposal: AgentProposal =
        decode_record(proposal_row.get("record_json"), "Agent proposal")?;
    if proposal.version != request.expected_proposal_version {
        return Err(version_conflict(
            "Agent proposal",
            request.expected_proposal_version,
            proposal.version,
        ));
    }
    if proposal.status != AgentProposalStatus::Submitted {
        return Err(ApiError::conflict(
            "proposal_already_decided",
            "Agent proposal is no longer submitted",
        ));
    }
    proposal.status = request.decision.proposal_status();
    proposal.version += 1;
    proposal.updated_at = Utc::now();
    let decision = HumanDecision {
        decision_id: request.decision_id,
        paper_project_id: paper_id,
        proposal_id: request.proposal_id,
        player_id: assertion.player_id,
        decision: request.decision.clone(),
        reason_hash: request.reason_hash.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
        version: 1,
    };
    sqlx::query(
        "insert into hepta_human_decisions (
            decision_id,paper_project_id,proposal_id,player_id,decision,reason_hash,
            signing_key_id,signing_public_key,signing_public_key_hash,signature,
            version,record_json,signed_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11::jsonb,$12)",
    )
    .bind(decision.decision_id)
    .bind(paper_id)
    .bind(decision.proposal_id)
    .bind(decision.player_id)
    .bind(decision.decision.as_str())
    .bind(&decision.reason_hash)
    .bind(&decision.signing_key_id)
    .bind(&decision.signing_public_key)
    .bind(&decision.signing_public_key_hash)
    .bind(&decision.signature)
    .bind(
        serde_json::to_value(&decision)
            .map_err(|error| ApiError::internal(format!("encode human decision: {error}")))?,
    )
    .bind(signed_time(decision.signed_at_unix)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("human_decision_exists", "human decision already exists")
        }
        _ => ApiError::database(error),
    })?;
    let updated = sqlx::query(
        "update hepta_agent_proposals set status=$1,version=$2,record_json=$3::jsonb,updated_at=$4
         where proposal_id=$5 and paper_project_id=$6 and version=$7 and status='submitted'",
    )
    .bind(proposal.status.as_str())
    .bind(
        i64::try_from(proposal.version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .bind(
        serde_json::to_value(&proposal).map_err(|error| {
            ApiError::internal(format!("encode decided Agent proposal: {error}"))
        })?,
    )
    .bind(proposal.updated_at)
    .bind(proposal.proposal_id)
    .bind(paper_id)
    .bind(
        i64::try_from(request.expected_proposal_version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "proposal_concurrent_decision",
            "Agent proposal changed before this decision committed",
        ));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.human_decision.created.v1",
        paper_id,
        decision.decision_id,
        1,
        json!({"decision_id":decision.decision_id,"proposal_id":decision.proposal_id,"decision":decision.decision,"proposal_version":proposal.version}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(decision.decision_id),
        StatusCode::CREATED,
        &decision,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(decision)))
}

fn validate_section_revision_request(
    request: &CreateSectionRevisionRequest,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_section_key(&request.section_key)?;
    validate_digest_v2("patch_hash", &request.patch_hash)?;
    if request.fencing_token == 0 || request.section_revision_id == request.parent_revision_id {
        return Err(ApiError::bad_request(
            "invalid_section_revision_lineage",
            "section revision requires a nonzero fencing token and may not self-reference",
        ));
    }
    Ok(())
}

async fn create_section_revision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateSectionRevisionRequest>,
) -> Result<(StatusCode, Json<SectionRevision>), ApiError> {
    const OPERATION: &str = "create_section_revision_v3";
    validate_section_revision_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/section-revisions");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let head = memory
            .collaboration
            .section_heads
            .get(&(paper_id, request.section_key.clone()))
            .ok_or_else(|| {
                ApiError::conflict(
                    "section_head_not_initialized",
                    "section head is not initialized",
                )
            })?;
        if head.current_head_revision_id != request.parent_revision_id
            || head.fencing_token != request.fencing_token
        {
            return Err(ApiError::conflict(
                "stale_section_parent",
                "section parent or fencing token is not the current head",
            ));
        }
        let lease = memory
            .collaboration
            .leases
            .get(&request.lease_id)
            .filter(|lease| {
                lease.paper_project_id == paper_id
                    && lease.section_key == request.section_key
                    && lease.fencing_token == request.fencing_token
                    && lease.status == SectionLeaseStatus::Active
                    && lease.holder_player_id == assertion.player_id
                    && lease.expires_at > now
            })
            .ok_or_else(|| {
                ApiError::conflict(
                    "invalid_or_expired_section_lease",
                    "section revision requires the asserted player's current unexpired lease",
                )
            })?;
        let proposal = memory
            .collaboration
            .proposals
            .get(&request.proposal_id)
            .filter(|proposal| {
                proposal.paper_project_id == paper_id
                    && proposal.section_key == request.section_key
                    && proposal.parent_revision_id == request.parent_revision_id
                    && proposal.status == AgentProposalStatus::Accepted
                    && proposal.artifact_manifest_id == request.patch_manifest_id
                    && proposal.payload_hash == request.patch_hash
                    && proposal.binding_id == lease.holder_binding_id
            })
            .ok_or_else(|| {
                ApiError::conflict(
                    "accepted_proposal_required",
                    "section revision must exactly bind an accepted proposal under the current lease",
                )
            })?;
        if memory
            .collaboration
            .artifact_manifests
            .get(&request.patch_manifest_id)
            .is_none_or(|manifest| manifest.paper_project_id != paper_id)
        {
            return Err(ApiError::bad_request(
                "cross_paper_or_missing_patch",
                "patch manifest is missing or belongs to another paper",
            ));
        }
        let revision = SectionRevision {
            section_revision_id: request.section_revision_id,
            paper_project_id: paper_id,
            section_key: request.section_key.clone(),
            parent_revision_id: request.parent_revision_id,
            proposal_id: proposal.proposal_id,
            lease_id: lease.lease_id,
            fencing_token: request.fencing_token,
            patch_manifest_id: request.patch_manifest_id,
            patch_hash: request.patch_hash.clone(),
            status: SectionRevisionStatus::Proposed,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        if memory
            .collaboration
            .section_revisions
            .insert(revision.section_revision_id, revision.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "section_revision_exists",
                "section_revision_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.section_revision.created.v1",
            paper_id,
            revision.section_revision_id,
            1,
            json!({"section_revision_id":revision.section_revision_id,"section_key":revision.section_key,"parent_revision_id":revision.parent_revision_id}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &revision,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(revision)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("paper-section:{paper_id}:{}", request.section_key))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
    let scope_row = sqlx::query(
        "select h.current_head_revision_id,h.fencing_token,
                l.record_json as lease_json,p.record_json as proposal_json
         from hepta_section_heads h
         join hepta_section_leases l
           on l.lease_id=$3 and l.paper_project_id=h.paper_project_id and l.section_key=h.section_key
         join hepta_agent_proposals p
           on p.proposal_id=$4 and p.paper_project_id=h.paper_project_id and p.section_key=h.section_key
         where h.paper_project_id=$1 and h.section_key=$2
         for update of h,l,p",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .bind(request.lease_id)
    .bind(request.proposal_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "section_revision_scope_missing",
            "section head, lease, or proposal is missing from this paper/section",
        )
    })?;
    if scope_row.get::<Uuid, _>("current_head_revision_id") != request.parent_revision_id
        || u64::try_from(scope_row.get::<i64, _>("fencing_token"))
            .map_err(|_| ApiError::internal("negative fencing token"))?
            != request.fencing_token
    {
        return Err(ApiError::conflict(
            "stale_section_parent",
            "section parent or fencing token is not the current head",
        ));
    }
    let lease: SectionLease = decode_record(scope_row.get("lease_json"), "section lease")?;
    let proposal: AgentProposal = decode_record(scope_row.get("proposal_json"), "Agent proposal")?;
    if lease.status != SectionLeaseStatus::Active
        || lease.holder_player_id != assertion.player_id
        || lease.fencing_token != request.fencing_token
        || lease.expires_at <= now
    {
        return Err(ApiError::conflict(
            "invalid_or_expired_section_lease",
            "section revision requires the asserted player's current unexpired lease",
        ));
    }
    if proposal.status != AgentProposalStatus::Accepted
        || proposal.parent_revision_id != request.parent_revision_id
        || proposal.artifact_manifest_id != request.patch_manifest_id
        || proposal.payload_hash != request.patch_hash
        || proposal.binding_id != lease.holder_binding_id
    {
        return Err(ApiError::conflict(
            "accepted_proposal_required",
            "section revision must exactly bind an accepted proposal under the current lease",
        ));
    }
    require_scoped_ids_postgres(
        &mut tx,
        "hepta_artifact_manifests",
        "manifest_id",
        paper_id,
        &[request.patch_manifest_id],
        "patch_manifest_id",
    )
    .await?;
    let parent_row = sqlx::query(
        "select ref_kind,section_key from hepta_collaboration_revision_refs
         where revision_id=$1 and paper_project_id=$2 for share",
    )
    .bind(request.parent_revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::bad_request(
            "cross_paper_or_missing_parent",
            "section parent is missing or belongs to another paper",
        )
    })?;
    let ref_kind: String = parent_row.get("ref_kind");
    let parent_section: Option<String> = parent_row.get("section_key");
    if ref_kind == "section" && parent_section.as_deref() != Some(&request.section_key) {
        return Err(ApiError::conflict(
            "cross_section_parent",
            "section revision parent belongs to another section",
        ));
    }
    let revision = SectionRevision {
        section_revision_id: request.section_revision_id,
        paper_project_id: paper_id,
        section_key: request.section_key.clone(),
        parent_revision_id: request.parent_revision_id,
        proposal_id: request.proposal_id,
        lease_id: request.lease_id,
        fencing_token: request.fencing_token,
        patch_manifest_id: request.patch_manifest_id,
        patch_hash: request.patch_hash.clone(),
        status: SectionRevisionStatus::Proposed,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(
        "insert into hepta_section_revisions (
            section_revision_id,paper_project_id,section_key,parent_revision_id,
            proposal_id,lease_id,fencing_token,patch_manifest_id,patch_hash,
            status,version,record_json,created_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,'proposed',1,$10::jsonb,$11,$11)",
    )
    .bind(revision.section_revision_id)
    .bind(paper_id)
    .bind(&revision.section_key)
    .bind(revision.parent_revision_id)
    .bind(revision.proposal_id)
    .bind(revision.lease_id)
    .bind(
        i64::try_from(revision.fencing_token)
            .map_err(|_| ApiError::internal("fencing token overflow"))?,
    )
    .bind(revision.patch_manifest_id)
    .bind(&revision.patch_hash)
    .bind(
        serde_json::to_value(&revision)
            .map_err(|error| ApiError::internal(format!("encode section revision: {error}")))?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("section_revision_exists", "section revision already exists")
        }
        _ => ApiError::database(error),
    })?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.section_revision.created.v1",
        paper_id,
        revision.section_revision_id,
        1,
        json!({"section_revision_id":revision.section_revision_id,"section_key":revision.section_key,"parent_revision_id":revision.parent_revision_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(revision.section_revision_id),
        StatusCode::CREATED,
        &revision,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(revision)))
}

fn verify_section_review(
    paper_id: Uuid,
    section_revision_id: Uuid,
    player_id: Uuid,
    request: &CreateSectionReviewRequest,
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("review_hash", &request.review_hash)?;
    signed_time(request.signed_at_unix)?;
    let signing = SectionReviewSigningV1 {
        schema: SECTION_REVIEW_V1.to_string(),
        review_id: request.review_id,
        paper_project_id: paper_id,
        section_revision_id,
        reviewer_player_id: player_id,
        verdict: request.verdict.as_str().to_string(),
        review_hash: request.review_hash.clone(),
        expected_revision_version: request.expected_revision_version,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let bytes = section_review_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_section_review", message))?;
    verifying_key
        .verify(&bytes, &decode_signature(&request.signature)?)
        .map_err(|_| {
            ApiError::forbidden(
                "section_review_signature_failed",
                "section review signature verification failed",
            )
        })
}

async fn create_section_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, section_revision_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<CreateSectionReviewRequest>,
) -> Result<(StatusCode, Json<SectionReview>), ApiError> {
    const OPERATION: &str = "create_section_review_v3";
    let path =
        format!("/v2/hepta/papers/{paper_id}/section-revisions/{section_revision_id}/reviews");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionReview)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let verifying_key = active_human_key_memory(
            &memory,
            assertion.player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
            request.signed_at_unix,
        )?;
        verify_section_review(
            paper_id,
            section_revision_id,
            assertion.player_id,
            &request,
            &verifying_key,
        )?;
        let revision_snapshot = memory
            .collaboration
            .section_revisions
            .get(&section_revision_id)
            .cloned()
            .filter(|revision| revision.paper_project_id == paper_id)
            .ok_or_else(|| {
                ApiError::bad_request(
                    "cross_paper_or_missing_section_revision",
                    "section revision is missing or belongs to another paper",
                )
            })?;
        let lease = memory
            .collaboration
            .leases
            .get(&revision_snapshot.lease_id)
            .ok_or_else(|| ApiError::internal("section revision lease is missing"))?;
        if lease.holder_player_id == assertion.player_id {
            return Err(ApiError::forbidden(
                "independent_review_required",
                "section revision must be reviewed by a different human player",
            ));
        }
        if revision_snapshot.version != request.expected_revision_version {
            return Err(version_conflict(
                "section revision",
                request.expected_revision_version,
                revision_snapshot.version,
            ));
        }
        if revision_snapshot.status != SectionRevisionStatus::Proposed {
            return Err(ApiError::conflict(
                "section_revision_already_reviewed",
                "section revision is no longer awaiting review",
            ));
        }
        if memory.collaboration.section_reviews.values().any(|review| {
            review.section_revision_id == section_revision_id
                && review.reviewer_player_id == assertion.player_id
        }) {
            return Err(ApiError::conflict(
                "section_review_exists",
                "player already reviewed this section revision",
            ));
        }
        let revision = memory
            .collaboration
            .section_revisions
            .get_mut(&section_revision_id)
            .expect("validated section revision exists");
        revision.status = request.verdict.revision_status();
        revision.version += 1;
        revision.updated_at = Utc::now();
        let revision_version = revision.version;
        let review = SectionReview {
            review_id: request.review_id,
            paper_project_id: paper_id,
            section_revision_id,
            reviewer_player_id: assertion.player_id,
            verdict: request.verdict.clone(),
            review_hash: request.review_hash.clone(),
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            signed_at_unix: request.signed_at_unix,
            signature: request.signature.clone(),
            version: 1,
        };
        if memory
            .collaboration
            .section_reviews
            .insert(review.review_id, review.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "section_review_exists",
                "review_id already exists",
            ));
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.section_review.created.v1",
            paper_id,
            review.review_id,
            1,
            json!({"review_id":review.review_id,"section_revision_id":review.section_revision_id,"verdict":review.verdict,"revision_version":revision_version}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &review,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(review)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionReview)?;
    let verifying_key = active_human_key_postgres(
        &mut tx,
        assertion.player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
        request.signed_at_unix,
    )
    .await?;
    verify_section_review(
        paper_id,
        section_revision_id,
        assertion.player_id,
        &request,
        &verifying_key,
    )?;
    let revision_row = sqlx::query(
        "select r.record_json,l.holder_player_id from hepta_section_revisions r
         join hepta_section_leases l
           on l.lease_id=r.lease_id and l.paper_project_id=r.paper_project_id
         where r.section_revision_id=$1 and r.paper_project_id=$2 for update of r",
    )
    .bind(section_revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::bad_request(
            "cross_paper_or_missing_section_revision",
            "section revision is missing or belongs to another paper",
        )
    })?;
    if revision_row.get::<Uuid, _>("holder_player_id") == assertion.player_id {
        return Err(ApiError::forbidden(
            "independent_review_required",
            "section revision must be reviewed by a different human player",
        ));
    }
    let mut revision: SectionRevision =
        decode_record(revision_row.get("record_json"), "section revision")?;
    if revision.version != request.expected_revision_version {
        return Err(version_conflict(
            "section revision",
            request.expected_revision_version,
            revision.version,
        ));
    }
    if revision.status != SectionRevisionStatus::Proposed {
        return Err(ApiError::conflict(
            "section_revision_already_reviewed",
            "section revision is no longer awaiting review",
        ));
    }
    revision.status = request.verdict.revision_status();
    revision.version += 1;
    revision.updated_at = Utc::now();
    let review = SectionReview {
        review_id: request.review_id,
        paper_project_id: paper_id,
        section_revision_id,
        reviewer_player_id: assertion.player_id,
        verdict: request.verdict.clone(),
        review_hash: request.review_hash.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        signed_at_unix: request.signed_at_unix,
        signature: request.signature.clone(),
        version: 1,
    };
    sqlx::query(
        "insert into hepta_section_reviews (
            review_id,paper_project_id,section_revision_id,reviewer_player_id,verdict,
            review_hash,signing_key_id,signing_public_key,signing_public_key_hash,
            signature,version,record_json,signed_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,1,$11::jsonb,$12)",
    )
    .bind(review.review_id)
    .bind(paper_id)
    .bind(section_revision_id)
    .bind(review.reviewer_player_id)
    .bind(review.verdict.as_str())
    .bind(&review.review_hash)
    .bind(&review.signing_key_id)
    .bind(&review.signing_public_key)
    .bind(&review.signing_public_key_hash)
    .bind(&review.signature)
    .bind(
        serde_json::to_value(&review)
            .map_err(|error| ApiError::internal(format!("encode section review: {error}")))?,
    )
    .bind(signed_time(review.signed_at_unix)?)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("section_review_exists", "section review already exists")
        }
        _ => ApiError::database(error),
    })?;
    let updated = sqlx::query(
        "update hepta_section_revisions set status=$1,version=$2,record_json=$3::jsonb,updated_at=$4
         where section_revision_id=$5 and paper_project_id=$6 and version=$7 and status='proposed'",
    )
    .bind(revision.status.as_str())
    .bind(i64::try_from(revision.version).map_err(|_| ApiError::internal("revision version overflow"))?)
    .bind(serde_json::to_value(&revision).map_err(|error| {
        ApiError::internal(format!("encode reviewed section revision: {error}"))
    })?)
    .bind(revision.updated_at)
    .bind(section_revision_id)
    .bind(paper_id)
    .bind(i64::try_from(request.expected_revision_version).map_err(|_| ApiError::internal("revision version overflow"))?)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "section_review_race",
            "section revision changed before review committed",
        ));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.section_review.created.v1",
        paper_id,
        review.review_id,
        1,
        json!({"review_id":review.review_id,"section_revision_id":review.section_revision_id,"verdict":review.verdict,"revision_version":revision.version}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(review.review_id),
        StatusCode::CREATED,
        &review,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(review)))
}

fn verify_section_merge(
    paper_id: Uuid,
    section_key: &str,
    player_id: Uuid,
    request: &CreateSectionMergeRequest,
    verifying_key: &ed25519_dalek::VerifyingKey,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    signed_time(request.merged_at_unix)?;
    if request.fencing_token == 0
        || request.merged_section_revision_id != request.section_revision_id
        || request.parent_revision_id == request.section_revision_id
    {
        return Err(ApiError::bad_request(
            "invalid_section_merge_lineage",
            "merge must advance to exactly the reviewed section revision without self-parenting",
        ));
    }
    let signing = SectionMergeSigningV1 {
        schema: SECTION_MERGE_V1.to_string(),
        merge_id: request.merge_id,
        paper_project_id: paper_id,
        section_key: section_key.to_string(),
        section_revision_id: request.section_revision_id,
        parent_revision_id: request.parent_revision_id,
        merged_section_revision_id: request.merged_section_revision_id,
        lease_id: request.lease_id,
        fencing_token: request.fencing_token,
        merged_by_player_id: player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        merged_at_unix: request.merged_at_unix,
    };
    let bytes = section_merge_signing_bytes(&signing)
        .map_err(|message| ApiError::bad_request("invalid_section_merge", message))?;
    verifying_key
        .verify(&bytes, &decode_signature(&request.signature)?)
        .map_err(|_| {
            ApiError::forbidden(
                "section_merge_signature_failed",
                "section merge signature verification failed",
            )
        })
}

async fn create_section_merge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateSectionMergeRequest>,
) -> Result<(StatusCode, Json<SectionMerge>), ApiError> {
    const OPERATION: &str = "create_section_merge_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    let path = format!("/v2/hepta/papers/{paper_id}/section-merges");
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let merged_at = signed_time(request.merged_at_unix)?;
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let (paper, _) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let revision = memory
            .collaboration
            .section_revisions
            .get(&request.section_revision_id)
            .cloned()
            .filter(|revision| revision.paper_project_id == paper_id)
            .ok_or_else(|| {
                ApiError::bad_request(
                    "cross_paper_or_missing_section_revision",
                    "section revision is missing or belongs to another paper",
                )
            })?;
        let verifying_key = active_human_key_memory(
            &memory,
            assertion.player_id,
            &request.signing_key_id,
            &request.signing_public_key,
            &request.signing_public_key_hash,
            request.merged_at_unix,
        )?;
        verify_section_merge(
            paper_id,
            &revision.section_key,
            assertion.player_id,
            &request,
            &verifying_key,
        )?;
        let lease = memory
            .collaboration
            .leases
            .get(&request.lease_id)
            .cloned()
            .filter(|lease| {
                lease.paper_project_id == paper_id
                    && lease.section_key == revision.section_key
                    && lease.holder_player_id == assertion.player_id
                    && lease.fencing_token == request.fencing_token
                    && lease.status == SectionLeaseStatus::Active
                    && lease.expires_at > merged_at
            })
            .ok_or_else(|| {
                ApiError::conflict(
                    "invalid_or_expired_section_lease",
                    "merge requires the holder's current unexpired section lease",
                )
            })?;
        let head = memory
            .collaboration
            .section_heads
            .get(&(paper_id, revision.section_key.clone()))
            .cloned()
            .ok_or_else(|| ApiError::internal("section head is missing"))?;
        if revision.status != SectionRevisionStatus::Approved
            || revision.version != request.expected_revision_version
            || revision.parent_revision_id != request.parent_revision_id
            || revision.lease_id != lease.lease_id
            || revision.fencing_token != request.fencing_token
            || head.current_head_revision_id != request.parent_revision_id
            || head.fencing_token != request.fencing_token
        {
            return Err(ApiError::conflict(
                "stale_or_unapproved_section_merge",
                "merge revision, parent, lease, version, approval, or head is stale",
            ));
        }
        let merge = SectionMerge {
            merge_id: request.merge_id,
            paper_project_id: paper_id,
            section_key: revision.section_key.clone(),
            section_revision_id: request.section_revision_id,
            parent_revision_id: request.parent_revision_id,
            merged_section_revision_id: request.merged_section_revision_id,
            lease_id: request.lease_id,
            fencing_token: request.fencing_token,
            merged_by_player_id: assertion.player_id,
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            merged_at_unix: request.merged_at_unix,
            signature: request.signature.clone(),
            version: 1,
        };
        if memory
            .collaboration
            .section_merges
            .insert(merge.merge_id, merge.clone())
            .is_some()
        {
            return Err(ApiError::conflict(
                "section_merge_exists",
                "merge_id already exists",
            ));
        }
        let stored_revision = memory
            .collaboration
            .section_revisions
            .get_mut(&request.section_revision_id)
            .expect("validated revision exists");
        stored_revision.status = SectionRevisionStatus::Merged;
        stored_revision.version += 1;
        stored_revision.updated_at = merged_at;
        let stored_lease = memory
            .collaboration
            .leases
            .get_mut(&request.lease_id)
            .expect("validated lease exists");
        stored_lease.status = SectionLeaseStatus::Consumed;
        stored_lease.version += 1;
        stored_lease.updated_at = merged_at;
        let stored_head = memory
            .collaboration
            .section_heads
            .get_mut(&(paper_id, revision.section_key.clone()))
            .expect("validated head exists");
        stored_head.current_head_revision_id = request.section_revision_id;
        stored_head.version += 1;
        stored_head.updated_at = merged_at;
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.section_merge.created.v1",
            paper_id,
            merge.merge_id,
            1,
            json!({"merge_id":merge.merge_id,"section_key":merge.section_key,"parent_revision_id":merge.parent_revision_id,"merged_section_revision_id":merge.merged_section_revision_id}),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &merge,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::CREATED, Json(merge)));
    }
    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        tx.commit().await.map_err(ApiError::database)?;
        return decode_stored(replay);
    }
    let (paper, _) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::SectionDraft)?;
    let revision_row = sqlx::query(
        "select record_json from hepta_section_revisions
         where section_revision_id=$1 and paper_project_id=$2 for update",
    )
    .bind(request.section_revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::bad_request(
            "cross_paper_or_missing_section_revision",
            "section revision is missing or belongs to another paper",
        )
    })?;
    let mut revision: SectionRevision =
        decode_record(revision_row.get("record_json"), "section revision")?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("paper-section:{paper_id}:{}", revision.section_key))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let verifying_key = active_human_key_postgres(
        &mut tx,
        assertion.player_id,
        &request.signing_key_id,
        &request.signing_public_key,
        &request.signing_public_key_hash,
        request.merged_at_unix,
    )
    .await?;
    verify_section_merge(
        paper_id,
        &revision.section_key,
        assertion.player_id,
        &request,
        &verifying_key,
    )?;
    let lease_row = sqlx::query(
        "select record_json from hepta_section_leases
         where lease_id=$1 and paper_project_id=$2 for update",
    )
    .bind(request.lease_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::conflict("section_lease_missing", "section lease is missing"))?;
    let mut lease: SectionLease = decode_record(lease_row.get("record_json"), "section lease")?;
    let head_row = sqlx::query(
        "select current_head_revision_id,fencing_token,version from hepta_section_heads
         where paper_project_id=$1 and section_key=$2 for update",
    )
    .bind(paper_id)
    .bind(&revision.section_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let current_head: Uuid = head_row.get("current_head_revision_id");
    let head_token = u64::try_from(head_row.get::<i64, _>("fencing_token"))
        .map_err(|_| ApiError::internal("negative head fencing token"))?;
    if revision.status != SectionRevisionStatus::Approved
        || revision.version != request.expected_revision_version
        || revision.parent_revision_id != request.parent_revision_id
        || revision.lease_id != request.lease_id
        || revision.fencing_token != request.fencing_token
        || lease.section_key != revision.section_key
        || lease.holder_player_id != assertion.player_id
        || lease.fencing_token != request.fencing_token
        || lease.status != SectionLeaseStatus::Active
        || lease.expires_at <= merged_at
        || current_head != request.parent_revision_id
        || head_token != request.fencing_token
    {
        return Err(ApiError::conflict(
            "stale_or_unapproved_section_merge",
            "merge revision, parent, lease, version, approval, or head is stale",
        ));
    }
    let merge = SectionMerge {
        merge_id: request.merge_id,
        paper_project_id: paper_id,
        section_key: revision.section_key.clone(),
        section_revision_id: request.section_revision_id,
        parent_revision_id: request.parent_revision_id,
        merged_section_revision_id: request.merged_section_revision_id,
        lease_id: request.lease_id,
        fencing_token: request.fencing_token,
        merged_by_player_id: assertion.player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        merged_at_unix: request.merged_at_unix,
        signature: request.signature.clone(),
        version: 1,
    };
    sqlx::query(
        "insert into hepta_section_merges (
            merge_id,paper_project_id,section_key,section_revision_id,parent_revision_id,
            merged_section_revision_id,lease_id,fencing_token,merged_by_player_id,
            signing_key_id,signing_public_key,signing_public_key_hash,signature,
            version,record_json,merged_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,1,$14::jsonb,$15)",
    )
    .bind(merge.merge_id)
    .bind(paper_id)
    .bind(&merge.section_key)
    .bind(merge.section_revision_id)
    .bind(merge.parent_revision_id)
    .bind(merge.merged_section_revision_id)
    .bind(merge.lease_id)
    .bind(
        i64::try_from(merge.fencing_token)
            .map_err(|_| ApiError::internal("fencing token overflow"))?,
    )
    .bind(merge.merged_by_player_id)
    .bind(&merge.signing_key_id)
    .bind(&merge.signing_public_key)
    .bind(&merge.signing_public_key_hash)
    .bind(&merge.signature)
    .bind(
        serde_json::to_value(&merge)
            .map_err(|error| ApiError::internal(format!("encode section merge: {error}")))?,
    )
    .bind(merged_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => {
            ApiError::conflict("section_merge_exists", "section merge already exists")
        }
        _ => ApiError::database(error),
    })?;
    revision.status = SectionRevisionStatus::Merged;
    revision.version += 1;
    revision.updated_at = merged_at;
    lease.status = SectionLeaseStatus::Consumed;
    lease.version += 1;
    lease.updated_at = merged_at;
    sqlx::query(
        "update hepta_section_revisions set status='merged',version=$1,record_json=$2::jsonb,updated_at=$3
         where section_revision_id=$4 and version=$5 and status='approved'",
    )
    .bind(i64::try_from(revision.version).map_err(|_| ApiError::internal("revision version overflow"))?)
    .bind(serde_json::to_value(&revision).map_err(|error| {
        ApiError::internal(format!("encode merged section revision: {error}"))
    })?)
    .bind(merged_at)
    .bind(revision.section_revision_id)
    .bind(i64::try_from(request.expected_revision_version).map_err(|_| ApiError::internal("revision version overflow"))?)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    sqlx::query(
        "update hepta_section_leases set status='consumed',version=$1,record_json=$2::jsonb,updated_at=$3
         where lease_id=$4 and status='active' and fencing_token=$5",
    )
    .bind(i64::try_from(lease.version).map_err(|_| ApiError::internal("lease version overflow"))?)
    .bind(serde_json::to_value(&lease).map_err(|error| {
        ApiError::internal(format!("encode consumed section lease: {error}"))
    })?)
    .bind(merged_at)
    .bind(lease.lease_id)
    .bind(i64::try_from(lease.fencing_token).map_err(|_| ApiError::internal("fencing token overflow"))?)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    sqlx::query(
        "update hepta_section_heads
         set current_head_revision_id=$1,version=version+1,updated_at=$2
         where paper_project_id=$3 and section_key=$4
           and current_head_revision_id=$5 and fencing_token=$6",
    )
    .bind(request.section_revision_id)
    .bind(merged_at)
    .bind(paper_id)
    .bind(&revision.section_key)
    .bind(request.parent_revision_id)
    .bind(
        i64::try_from(request.fencing_token)
            .map_err(|_| ApiError::internal("fencing token overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.section_merge.created.v1",
        paper_id,
        merge.merge_id,
        1,
        json!({"merge_id":merge.merge_id,"section_key":merge.section_key,"parent_revision_id":merge.parent_revision_id,"merged_section_revision_id":merge.merged_section_revision_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(merge.merge_id),
        StatusCode::CREATED,
        &merge,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(merge)))
}

fn member_session_access(
    set: &ResearchSessionAuthorizationSetV1,
    player_id: Uuid,
    completion_received: bool,
) -> Result<Option<MemberResearchSessionAccess>, ApiError> {
    let Some(member) = set
        .members
        .iter()
        .find(|member| member.player_id == player_id)
    else {
        return Ok(None);
    };
    let authorization_id = Uuid::parse_str(&member.authorization.claim.authorization_id)
        .map_err(|_| ApiError::internal("stored research-session authorization_id is not UUID"))?;
    Ok(Some(MemberResearchSessionAccess {
        logical_session_id: set.session_id.clone(),
        authorization_set_id: set.authorization_set_id,
        roster_version: set.roster_version,
        authorization_id,
        status: set.status.clone(),
        issued_at: set.issued_at,
        expires_at: set.expires_at,
        consumed_at: member.consumed_at,
        nakama_completion_received: completion_received,
    }))
}

async fn get_paper_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
) -> Result<Json<PaperRoomReadModel>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/room");
    let assertion =
        require_registered_player_read(&state, &headers, "get_paper_room_v3", &path).await?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        let mut team_member_acceptances: Vec<_> = memory
            .team_acceptances
            .values()
            .filter(|item| item.team_id == team.team_id)
            .cloned()
            .collect();
        team_member_acceptances.sort_by_key(|item| (item.participant_slot, item.acceptance_id));
        let mut work_items: Vec<_> = memory
            .work_items
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        work_items.sort_by_key(|item| (item.created_at, item.work_item_id));
        let mut paper_revisions: Vec<_> = memory
            .revisions
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        paper_revisions.sort_by_key(|item| (item.revision_number, item.revision_id));
        let mut authorship_consents: Vec<_> = memory
            .consents
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        authorship_consents.sort_by_key(|item| (item.signed_at, item.consent_id));
        let joint_submission = memory
            .submissions
            .values()
            .find(|item| item.paper_project_id == paper_id)
            .cloned();
        let mut member_research_sessions = Vec::new();
        for set in memory
            .research_session_authorization_sets
            .values()
            .filter(|set| set.paper_project_id == paper_id)
        {
            if let Some(access) = member_session_access(
                set,
                assertion.player_id,
                memory
                    .research_session_completions
                    .contains_key(&(set.session_id.clone(), set.roster_version)),
            )? {
                member_research_sessions.push(access);
            }
        }
        member_research_sessions
            .sort_by_key(|item| (item.logical_session_id.clone(), item.roster_version));
        let mut artifact_manifests: Vec<_> = memory
            .collaboration
            .artifact_manifests
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        artifact_manifests.sort_by_key(|item| (item.created_at, item.manifest_id));
        let mut revision_artifact_bindings: Vec<_> = memory
            .collaboration
            .revision_artifact_bindings
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        revision_artifact_bindings.sort_by_key(|item| (item.created_at, item.revision_id));
        let mut evidence_cards: Vec<_> = memory
            .collaboration
            .evidence_cards
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        evidence_cards.sort_by_key(|item| (item.created_at, item.evidence_card_id));
        let mut citations: Vec<_> = memory
            .collaboration
            .citations
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        citations.sort_by_key(|item| (item.created_at, item.citation_id));
        let mut experiment_plans: Vec<_> = memory
            .collaboration
            .experiment_plans
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        experiment_plans.sort_by_key(|item| (item.created_at, item.experiment_plan_id));
        let mut runs: Vec<_> = memory
            .collaboration
            .runs
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        runs.sort_by_key(|item| (item.created_at, item.run_record_id));
        let mut figures: Vec<_> = memory
            .collaboration
            .figures
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        figures.sort_by_key(|item| (item.created_at, item.figure_lineage_id));
        let mut claims: Vec<_> = memory
            .collaboration
            .claims
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        claims.sort_by_key(|item| (item.created_at, item.claim_id));
        let mut section_heads: Vec<_> = memory
            .collaboration
            .section_heads
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        section_heads.sort_by(|left, right| left.section_key.cmp(&right.section_key));
        let mut leases: Vec<_> = memory
            .collaboration
            .leases
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        leases.sort_by_key(|item| (item.acquired_at, item.lease_id));
        let mut proposals: Vec<_> = memory
            .collaboration
            .proposals
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        proposals.sort_by_key(|item| (item.updated_at, item.proposal_id));
        let mut decisions: Vec<_> = memory
            .collaboration
            .decisions
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        decisions.sort_by_key(|item| item.decision_id);
        let mut section_revisions: Vec<_> = memory
            .collaboration
            .section_revisions
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        section_revisions.sort_by_key(|item| (item.created_at, item.section_revision_id));
        let mut section_reviews: Vec<_> = memory
            .collaboration
            .section_reviews
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        section_reviews.sort_by_key(|item| item.review_id);
        let mut section_merges: Vec<_> = memory
            .collaboration
            .section_merges
            .values()
            .filter(|item| item.paper_project_id == paper_id)
            .cloned()
            .collect();
        section_merges.sort_by_key(|item| (item.merged_at_unix, item.merge_id));
        let last_event_cursor = memory
            .collaboration
            .events
            .iter()
            .filter(|event| event.paper_project_id == paper_id)
            .map(|event| event.cursor)
            .max()
            .unwrap_or(0);
        return Ok(Json(PaperRoomReadModel {
            paper,
            team,
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
        }));
    }
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    sqlx::query("set transaction isolation level repeatable read")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    let team_member_acceptances = sqlx::query(
        "select record_json from hepta_research_team_member_acceptances
         where team_id=$1 order by participant_slot,acceptance_id",
    )
    .bind(team.team_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "team member acceptance"))
    .collect::<Result<Vec<TeamMemberAcceptance>, _>>()?;
    let work_items = sqlx::query(
        "select record_json from hepta_paper_work_items
         where paper_project_id=$1 order by created_at,work_item_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "work item"))
    .collect::<Result<Vec<WorkItem>, _>>()?;
    let paper_revisions = sqlx::query(
        "select record_json from hepta_paper_revisions
         where paper_project_id=$1 order by revision_number,revision_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "paper revision"))
    .collect::<Result<Vec<PaperRevision>, _>>()?;
    let authorship_consents = sqlx::query(
        "select record_json from hepta_authorship_consents
         where paper_project_id=$1 order by signed_at,consent_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "authorship consent"))
    .collect::<Result<Vec<AuthorshipConsent>, _>>()?;
    let joint_submission = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id=$1 order by created_at desc limit 1",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .map(|row| decode_record(row.get("record_json"), "joint paper submission"))
    .transpose()?;
    let authorization_rows = sqlx::query(
        "select s.record_json,
                exists(select 1 from hepta_nakama_research_session_completions c
                       where c.authorization_set_id=s.authorization_set_id) as completion_received
         from hepta_research_session_authorization_sets s
         join hepta_research_session_authorizations a
           on a.authorization_set_id=s.authorization_set_id and a.player_id=$2
         where s.paper_project_id=$1
         order by s.session_id,s.roster_version",
    )
    .bind(paper_id)
    .bind(assertion.player_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let mut member_research_sessions = Vec::with_capacity(authorization_rows.len());
    for row in authorization_rows {
        let set: ResearchSessionAuthorizationSetV1 =
            decode_record(row.get("record_json"), "research-session authorization set")?;
        if let Some(access) =
            member_session_access(&set, assertion.player_id, row.get("completion_received"))?
        {
            member_research_sessions.push(access);
        }
    }
    macro_rules! records {
        ($table:literal, $kind:literal, $ty:ty) => {{
            sqlx::query(concat!(
                "select record_json from ",
                $table,
                " where paper_project_id=$1 order by created_at"
            ))
            .bind(paper_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(|row| decode_record::<$ty>(row.get("record_json"), $kind))
            .collect::<Result<Vec<_>, _>>()?
        }};
    }
    let artifact_manifests = records!(
        "hepta_artifact_manifests",
        "artifact manifest",
        ArtifactManifest
    );
    let revision_artifact_bindings = records!(
        "hepta_paper_revision_artifact_bindings",
        "paper revision artifact binding",
        PaperRevisionArtifactBinding
    );
    let evidence_cards = records!("hepta_evidence_cards", "evidence card", EvidenceCard);
    let citations = records!("hepta_citation_records", "citation", CitationRecord);
    let experiment_plans = records!("hepta_experiment_plans", "experiment plan", ExperimentPlan);
    let runs = records!("hepta_run_records", "run record", RunRecord);
    let figures = records!("hepta_figure_lineage", "figure lineage", FigureLineage);
    let claims = records!("hepta_claim_records", "claim", ClaimRecord);
    let leases = sqlx::query(
        "select record_json from hepta_section_leases
         where paper_project_id=$1 order by acquired_at,lease_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "section lease"))
    .collect::<Result<Vec<SectionLease>, _>>()?;
    let proposals = sqlx::query(
        "select record_json from hepta_agent_proposals where paper_project_id=$1 order by updated_at,proposal_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "Agent proposal"))
    .collect::<Result<Vec<AgentProposal>, _>>()?;
    let decisions = sqlx::query(
        "select record_json from hepta_human_decisions where paper_project_id=$1 order by signed_at,decision_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "human decision"))
    .collect::<Result<Vec<HumanDecision>, _>>()?;
    let section_revisions = records!(
        "hepta_section_revisions",
        "section revision",
        SectionRevision
    );
    let section_reviews = sqlx::query(
        "select record_json from hepta_section_reviews where paper_project_id=$1 order by signed_at,review_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "section review"))
    .collect::<Result<Vec<SectionReview>, _>>()?;
    let section_merges = sqlx::query(
        "select record_json from hepta_section_merges where paper_project_id=$1 order by merged_at,merge_id",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "section merge"))
    .collect::<Result<Vec<SectionMerge>, _>>()?;
    let section_heads = sqlx::query(
        "select paper_project_id,section_key,base_paper_revision_id,current_head_revision_id,
                fencing_token,version,updated_at
         from hepta_section_heads where paper_project_id=$1 order by section_key",
    )
    .bind(paper_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| {
        Ok(SectionHead {
            paper_project_id: row.get("paper_project_id"),
            section_key: row.get("section_key"),
            base_paper_revision_id: row.get("base_paper_revision_id"),
            current_head_revision_id: row.get("current_head_revision_id"),
            fencing_token: u64::try_from(row.get::<i64, _>("fencing_token"))
                .map_err(|_| ApiError::internal("negative section-head fencing token"))?,
            version: u64::try_from(row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("negative section-head version"))?,
            updated_at: row.get("updated_at"),
        })
    })
    .collect::<Result<Vec<_>, ApiError>>()?;
    let last_event_cursor = sqlx::query(
        "select coalesce(max(cursor),0)::bigint as cursor from hepta_paper_room_events where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .get::<i64, _>("cursor");
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(PaperRoomReadModel {
        paper,
        team,
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
        last_event_cursor: u64::try_from(last_event_cursor)
            .map_err(|_| ApiError::internal("negative paper-room cursor"))?,
    }))
}

async fn list_paper_room_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Query(query): Query<EventCursorQuery>,
) -> Result<Json<Vec<PaperRoomEvent>>, ApiError> {
    let path = format!(
        "/v2/hepta/papers/{paper_id}/events?after_cursor={}",
        query.after_cursor
    );
    let assertion =
        require_registered_player_read(&state, &headers, "list_paper_room_events_v3", &path)
            .await?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        paper_and_team_memory(&memory, paper_id, &assertion)?;
        let events = memory
            .collaboration
            .events
            .iter()
            .filter(|event| event.paper_project_id == paper_id && event.cursor > query.after_cursor)
            .take(1_000)
            .cloned()
            .collect();
        return Ok(Json(events));
    }
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    sqlx::query("set transaction isolation level repeatable read")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    let after_cursor = i64::try_from(query.after_cursor)
        .map_err(|_| ApiError::bad_request("invalid_cursor", "cursor exceeds PostgreSQL bigint"))?;
    let events = sqlx::query(
        "select cursor,event_id,paper_project_id,event_type,aggregate_id,
                aggregate_version,payload,occurred_at
         from hepta_paper_room_events
         where paper_project_id=$1 and cursor>$2 order by cursor limit 1000",
    )
    .bind(paper_id)
    .bind(after_cursor)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| {
        Ok(PaperRoomEvent {
            cursor: u64::try_from(row.get::<i64, _>("cursor"))
                .map_err(|_| ApiError::internal("negative paper-room cursor"))?,
            event_id: row.get("event_id"),
            paper_project_id: row.get("paper_project_id"),
            event_type: row.get("event_type"),
            aggregate_id: row.get("aggregate_id"),
            aggregate_version: u64::try_from(row.get::<i64, _>("aggregate_version"))
                .map_err(|_| ApiError::internal("negative aggregate version"))?,
            payload: row.get("payload"),
            occurred_at: row.get("occurred_at"),
        })
    })
    .collect::<Result<Vec<_>, ApiError>>()?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(events))
}

#[cfg(test)]
#[path = "paper_collaboration_v3_tests.rs"]
mod tests;
