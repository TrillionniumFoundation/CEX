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
    agent_proposal_v2_signing_bytes, human_decision_signing_bytes,
    human_evidence_verification_signing_bytes, section_materialization_root,
    section_merge_signing_bytes, section_review_signing_bytes, AgentProposalSigningV2,
    HumanDecisionSigningV1, HumanEvidenceVerificationSigningV1, SectionMaterializationDescriptorV1,
    SectionMaterializationEntryV1, SectionMergeSigningV1, SectionReviewSigningV1,
    AGENT_PROPOSAL_V2, HUMAN_DECISION_V1, HUMAN_EVIDENCE_VERIFICATION_V1,
    SECTION_MATERIALIZATION_V1, SECTION_MERGE_V1, SECTION_REVIEW_V1,
};

#[path = "paper_collaboration_v3/role_resources_v1.rs"]
mod role_resources_v1;
pub use role_resources_v1::*;
pub(super) use role_resources_v1::{
    role_resource_state_from_snapshot, validate_paper_role_resources,
};

#[path = "paper_collaboration_v3/matchmaking_party_v1.rs"]
mod matchmaking_party_v1;
use matchmaking_party_v1::{
    live_party_ticket_count, matchmaking_partition_compatible, matchmaking_partition_key,
    postgres_party_admission_lock_key,
};

pub const PAPER_COLLABORATION_PROTOCOL_V3: &str = "hepta.paper_raid.collaboration.v3";
pub const SOURCE_ARTIFACT_BUNDLE_SCHEMA_V1: &str = "paper-raid.artifact-bundle.v1";
pub const ARTIFACT_MANIFEST_BINDING_SCHEMA_V1: &str =
    "hepta.paper_raid.artifact_manifest_binding.v1";
pub const REVIEW_READY_ARTIFACT_MANIFEST_BINDING_SCHEMA_V1: &str =
    "hepta.paper_raid.review_ready_artifact_manifest_binding.v1";
pub const REVIEW_READY_ARTIFACT_ASSEMBLY_SCHEMA_V1: &str =
    "hepta.paper_raid.review_ready_artifact_assembly.v1";
pub const ARTIFACT_BUNDLE_ADAPTER_CONTRACT_HASH_V1: &str =
    "sha256:aba8fd6d1059c59f63cdb258a2e507de1bed3ff74f935b7dd0214e4640ad9bb6";
pub const ARTIFACT_BUNDLE_ADAPTER_SOURCE_REVISION: &str =
    "61c9ffd0b410faed023b68a604e4d0906c3006f8";
const MAX_ALPHA_MATCH_CANDIDATES: usize = 2_048;
const MATCHMAKING_TICKET_TTL_SECONDS: i64 = 30 * 60;
const TEAM_PROPOSAL_RESPONSE_TTL_SECONDS: i64 = 5 * 60;
pub const MATCHMAKING_QUEUE_HINT_SCHEMA_V1: &str = "hepta.paper_raid.matchmaking_queue_hint.v1";
pub const MATCHMAKING_SOLVER_VERSION_V2: &str = "hepta.paper_raid.alpha_matcher.v2";
const AUTOMATIC_CHALLENGE_EXPIRY_OPERATION: &str = "materialize_paper_challenge_expiry_v1";
const AUTOMATIC_CHALLENGE_EXPIRY_EVENT: &str = "hepta.paper_raid.challenge_outcome.terminal.v1";
const AUTOMATIC_CHALLENGE_EXPIRY_REASON: &str = "challenge_grace_deadline_elapsed";
const AUTOMATIC_REWORK_EXPIRY_REASON: &str = "rework_window_elapsed";

/// Read one database-authoritative, microsecond-precision timestamp and reuse
/// it for both relational columns and their immutable JSON projection.
pub(super) async fn postgres_transaction_now(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<DateTime<Utc>, ApiError> {
    sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::database)
}

#[derive(Clone, Default)]
pub(super) struct CollaborationMemory {
    tickets: HashMap<Uuid, MatchmakingTicket>,
    pub(super) team_proposals: HashMap<Uuid, TeamProposal>,
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

#[cfg(test)]
impl CollaborationMemory {
    pub(super) fn expire_section_lease_for_agent_authority_test(
        &mut self,
        lease_id: Uuid,
        expires_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) {
        let lease = self
            .leases
            .get_mut(&lease_id)
            .expect("memory section lease for Agent authority test");
        lease.expires_at = expires_at;
        lease.updated_at = updated_at;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CollaborationPhaseGateFacts {
    pub artifact_manifest: bool,
    pub evidence_card: bool,
    pub citation: bool,
    pub experiment_plan: bool,
    pub retained_run: bool,
    pub claim: bool,
    pub section_revision: bool,
    pub approving_section_review: bool,
    pub section_merge: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CollaborationPhaseGateCounts {
    pub artifact_manifests: u32,
    pub evidence_cards: u32,
    pub citations: u32,
    pub experiment_plans: u32,
    pub retained_runs: u32,
    pub successful_runs: u32,
    pub retained_failed_runs: u32,
    pub claims: u32,
    pub section_revisions: u32,
    pub approving_section_reviews: u32,
    pub section_merges: u32,
}

pub(super) fn collaboration_phase_gate_counts_memory(
    memory: &CollaborationMemory,
    paper_id: Uuid,
) -> CollaborationPhaseGateCounts {
    CollaborationPhaseGateCounts {
        artifact_manifests: memory
            .artifact_manifests
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        evidence_cards: memory
            .evidence_cards
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        citations: memory
            .citations
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        experiment_plans: memory
            .experiment_plans
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        retained_runs: memory
            .runs
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        successful_runs: memory
            .runs
            .values()
            .filter(|record| {
                record.paper_project_id == paper_id && record.status == RunStatus::Succeeded
            })
            .count() as u32,
        retained_failed_runs: memory
            .runs
            .values()
            .filter(|record| {
                record.paper_project_id == paper_id
                    && record.status == RunStatus::Failed
                    && record.failure_hash.is_some()
            })
            .count() as u32,
        claims: memory
            .claims
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        section_revisions: memory
            .section_revisions
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
        approving_section_reviews: memory
            .section_reviews
            .values()
            .filter(|record| {
                record.paper_project_id == paper_id
                    && record.verdict == SectionReviewVerdict::Approve
            })
            .count() as u32,
        section_merges: memory
            .section_merges
            .values()
            .filter(|record| record.paper_project_id == paper_id)
            .count() as u32,
    }
}

pub(super) fn collaboration_phase_gate_facts_memory(
    memory: &CollaborationMemory,
    paper_id: Uuid,
) -> CollaborationPhaseGateFacts {
    CollaborationPhaseGateFacts {
        artifact_manifest: memory
            .artifact_manifests
            .values()
            .any(|record| record.paper_project_id == paper_id),
        evidence_card: memory
            .evidence_cards
            .values()
            .any(|record| record.paper_project_id == paper_id),
        citation: memory
            .citations
            .values()
            .any(|record| record.paper_project_id == paper_id),
        experiment_plan: memory
            .experiment_plans
            .values()
            .any(|record| record.paper_project_id == paper_id),
        retained_run: memory
            .runs
            .values()
            .any(|record| record.paper_project_id == paper_id),
        claim: memory
            .claims
            .values()
            .any(|record| record.paper_project_id == paper_id),
        section_revision: memory
            .section_revisions
            .values()
            .any(|record| record.paper_project_id == paper_id),
        approving_section_review: memory.section_reviews.values().any(|record| {
            record.paper_project_id == paper_id && record.verdict == SectionReviewVerdict::Approve
        }),
        section_merge: memory
            .section_merges
            .values()
            .any(|record| record.paper_project_id == paper_id),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchmakingTicketStatus {
    Queued,
    Matched,
    Consumed,
    Cancelled,
    Expired,
}

impl MatchmakingTicketStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Matched => "matched",
            Self::Consumed => "consumed",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party_code_hash: Option<String>,
    pub status: MatchmakingTicketStatus,
    pub matched_proposal_id: Option<Uuid>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_hint: Option<MatchmakingQueueHintV1>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchmakingTicketView {
    pub ticket_id: Uuid,
    pub player_id: Uuid,
    pub challenge_id: Uuid,
    pub requested_team_size: u32,
    pub roles: Vec<String>,
    pub availability_hash: String,
    #[serde(default)]
    pub private_party: bool,
    pub status: MatchmakingTicketStatus,
    pub matched_proposal_id: Option<Uuid>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_hint: Option<MatchmakingQueueHintV1>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<MatchmakingTicket> for MatchmakingTicketView {
    fn from(ticket: MatchmakingTicket) -> Self {
        Self {
            ticket_id: ticket.ticket_id,
            player_id: ticket.player_id,
            challenge_id: ticket.challenge_id,
            requested_team_size: ticket.requested_team_size,
            roles: ticket.roles,
            availability_hash: ticket.availability_hash,
            private_party: ticket.party_code_hash.is_some(),
            status: ticket.status,
            matched_proposal_id: ticket.matched_proposal_id,
            expires_at: ticket.expires_at,
            queue_hint: ticket.queue_hint,
            version: ticket.version,
            created_at: ticket.created_at,
            updated_at: ticket.updated_at,
        }
    }
}

fn matchmaking_ticket_created_event_payload(ticket: &MatchmakingTicket) -> Value {
    json!({
        "ticket_id": ticket.ticket_id,
        "challenge_id": ticket.challenge_id,
        "status": ticket.status,
        "matched_proposal_id": ticket.matched_proposal_id,
    })
}

fn matchmaking_ticket_cancelled_event_payload(ticket: &MatchmakingTicket) -> Value {
    json!({
        "ticket_id": ticket.ticket_id,
        "challenge_id": ticket.challenge_id,
        "player_id": ticket.player_id,
        "status": ticket.status,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchmakingQueueHintV1 {
    pub schema: String,
    pub state: String,
    pub queue_position: Option<u32>,
    pub compatible_pool_size: u32,
    pub compatible_players_needed: u32,
    pub missing_roles: Vec<String>,
    pub waited_seconds: u64,
    pub expires_in_seconds: u64,
    pub eta_seconds: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateMatchmakingTicketRequest {
    pub ticket_id: Uuid,
    pub challenge_id: Uuid,
    pub requested_team_size: u32,
    pub roles: Vec<String>,
    pub availability_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party_code_hash: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelMatchmakingTicketRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamProposalStatus {
    Proposed,
    Accepted,
    Materialized,
    Declined,
    Expired,
}

impl TeamProposalStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Materialized => "materialized",
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_preferences: Vec<TeamProposalSourcePreferenceV2>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_assignments: Vec<TeamProposalRoleAssignmentV2>,
    pub status: TeamProposalStatus,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamProposalSourcePreferenceV2 {
    pub ticket_id: Uuid,
    pub player_id: Uuid,
    pub roles: Vec<String>,
    pub availability_hash: String,
    pub private_party: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamProposalRoleAssignmentV2 {
    pub ticket_id: Uuid,
    pub player_id: Uuid,
    pub assigned_role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MatchmakingTicketResponse {
    pub ticket: MatchmakingTicketView,
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
    pub player_phase: String,
    pub outcome: PaperChallengeOutcomeV1,
    pub outcome_reason: Option<String>,
    pub terminal_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_resources: Option<RoleResourceStateV1>,
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
    pub ruleset_enforcement: crate::ChallengeRulesetEnforcementV1,
    pub ruleset: Option<crate::ChallengeRulesetV1>,
    pub status: crate::ChallengeStatus,
    pub created_at: DateTime<Utc>,
}

impl From<crate::ResearchChallenge> for PublicChallenge {
    fn from(challenge: crate::ResearchChallenge) -> Self {
        let ruleset_enforcement = if challenge.ruleset.is_some() {
            crate::ChallengeRulesetEnforcementV1::AuthoritativeV1
        } else {
            crate::ChallengeRulesetEnforcementV1::LegacyUnranked
        };
        Self {
            challenge_id: challenge.challenge_id,
            title: challenge.title,
            description: challenge.description,
            ruleset_version: challenge.ruleset_version,
            ruleset_hash: challenge.ruleset_hash,
            dataset_manifest_hash: challenge.dataset_manifest_hash,
            ruleset_enforcement,
            ruleset: challenge.ruleset,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_ready_assembly: Option<ReviewReadyArtifactAssemblyV1>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PaperRevisionSectionHeadBinding {
    pub section_key: String,
    pub base_paper_revision_id: Uuid,
    pub current_head_revision_id: Uuid,
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
    #[serde(default)]
    pub section_head_bindings: Vec<PaperRevisionSectionHeadBinding>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_ready_assembly: Option<ReviewReadyArtifactAssemblyV1>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactManifestPinV1 {
    pub manifest_id: Uuid,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewReadyArtifactAssemblyV1 {
    pub schema: String,
    pub draft: ArtifactManifestPinV1,
    pub frozen_evaluator: ArtifactManifestPinV1,
    pub dataset: ArtifactManifestPinV1,
    pub candidate: ArtifactManifestPinV1,
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

impl SectionLeaseStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Released => "released",
            Self::Expired => "expired",
            Self::Consumed => "consumed",
        }
    }
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
    pub lease_id: Uuid,
    pub lease_fencing_token: u64,
    pub expected_work_version: u64,
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
    pub lease_id: Uuid,
    pub lease_fencing_token: u64,
    pub expected_work_version: u64,
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
    pub author_raid_progress: AuthorRaidProgressV1,
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

pub const AUTHOR_RAID_PROGRESS_SCHEMA_V1: &str = "hepta.paper_raid.author_progress.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorRaidProgressV1 {
    pub schema: String,
    pub phase: PaperPhase,
    pub next_phase: Option<PaperPhase>,
    /// Player-facing semantic phase. The legacy `reproducing` wire value is
    /// retained in `phase` for stored V1 snapshots, but Author Raid never
    /// performs independent reproduction; that work belongs to Review Raid.
    pub player_phase: String,
    pub next_player_phase: Option<String>,
    pub objective: String,
    pub actor_role: String,
    pub role_enforcement: String,
    pub personal_objective: String,
    pub primary_actions: Vec<String>,
    pub shared_actions: Vec<String>,
    pub collaboration_overrides: Vec<String>,
    pub role_blockers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_resources: Option<RoleResourceProjectionV1>,
    pub actor_can_transition: bool,
    pub blockers: Vec<String>,
    pub next_actions: Vec<String>,
    pub transition_ready: bool,
}

#[allow(clippy::too_many_arguments)]
fn project_author_raid_progress(
    paper: &PaperProject,
    team: &ResearchTeam,
    actor_player_id: Uuid,
    work_items: &[WorkItem],
    paper_revisions: &[PaperRevision],
    revision_artifact_bindings: &[PaperRevisionArtifactBinding],
    authorship_consents: &[AuthorshipConsent],
    joint_submission: &Option<JointPaperSubmission>,
    artifact_manifests: &[ArtifactManifest],
    evidence_cards: &[EvidenceCard],
    citations: &[CitationRecord],
    experiment_plans: &[ExperimentPlan],
    runs: &[RunRecord],
    claims: &[ClaimRecord],
    section_heads: &[SectionHead],
    section_revisions: &[SectionRevision],
    section_reviews: &[SectionReview],
    section_merges: &[SectionMerge],
) -> AuthorRaidProgressV1 {
    let projection_now = Utc::now();
    let mut blockers = Vec::new();
    let mut next_actions = Vec::new();
    let (mut next_phase, mut objective) = match paper.phase {
        PaperPhase::Forming => {
            next_actions.push("transition_paper_project".to_string());
            (Some(PaperPhase::Preregistering), "open_preregistration")
        }
        PaperPhase::Preregistering => {
            if !work_items
                .iter()
                .any(|item| item.status != WorkItemStatus::Cancelled)
            {
                blockers.push("work_item_required".to_string());
                next_actions.push("create_paper_work_item".to_string());
            }
            if artifact_manifests.is_empty() {
                blockers.push("artifact_manifest_required".to_string());
                next_actions.push("register_artifact".to_string());
            }
            if experiment_plans.is_empty() {
                blockers.push("experiment_plan_required".to_string());
                next_actions.push("create_experiment_plan".to_string());
            }
            if blockers.is_empty() {
                next_actions.push("transition_paper_project".to_string());
            }
            (Some(PaperPhase::Researching), "lock_research_plan")
        }
        PaperPhase::Researching => {
            if evidence_cards.is_empty() {
                blockers.push("evidence_card_required".to_string());
                next_actions.push("create_evidence_card".to_string());
            }
            if citations.is_empty() {
                blockers.push("citation_required".to_string());
                next_actions.push("create_citation_record".to_string());
            }
            if claims.is_empty() {
                blockers.push("claim_required".to_string());
                next_actions.push("create_claim_record".to_string());
            }
            if blockers.is_empty() {
                next_actions.push("transition_paper_project".to_string());
            }
            (Some(PaperPhase::Experimenting), "bind_claims_to_evidence")
        }
        PaperPhase::Experimenting => {
            if runs.is_empty() {
                blockers.push("retained_run_required".to_string());
                next_actions.push("create_run_record".to_string());
            }
            if artifact_manifests.is_empty() {
                blockers.push("artifact_manifest_required".to_string());
                next_actions.push("register_artifact".to_string());
            }
            if blockers.is_empty() {
                next_actions.push("transition_paper_project".to_string());
            }
            (Some(PaperPhase::Drafting), "retain_results_and_artifacts")
        }
        PaperPhase::Drafting => {
            if work_items.iter().any(|item| {
                !matches!(
                    item.status,
                    WorkItemStatus::Accepted | WorkItemStatus::Cancelled
                )
            }) {
                blockers.push("work_items_must_be_accepted_or_cancelled".to_string());
                next_actions.push("transition_paper_work_item".to_string());
            }
            if section_revisions.is_empty() {
                blockers.push("section_revision_required".to_string());
                next_actions.push("create_section_revision".to_string());
            }
            if paper_revisions.is_empty() || paper.current_revision_id.is_none() {
                blockers.push("paper_revision_required".to_string());
                next_actions.push("create_paper_revision".to_string());
            }
            if blockers.is_empty() {
                next_actions.push("transition_paper_project".to_string());
            }
            (Some(PaperPhase::IntegrityReview), "assemble_draft")
        }
        PaperPhase::IntegrityReview => {
            if !section_reviews
                .iter()
                .any(|review| review.verdict == SectionReviewVerdict::Approve)
            {
                blockers.push("approving_section_review_required".to_string());
                next_actions.push("submit_review".to_string());
            }
            if section_merges.is_empty() {
                blockers.push("section_merge_required".to_string());
                next_actions.push("merge_section".to_string());
            }
            if blockers.is_empty() {
                next_actions.push("transition_paper_project".to_string());
            }
            (Some(PaperPhase::Reproducing), "resolve_integrity_review")
        }
        PaperPhase::Reproducing => {
            if paper.current_revision_id.is_none() {
                blockers.push("paper_revision_required".to_string());
                next_actions.push("create_paper_revision".to_string());
            } else if !paper_revision_covers_section_merges(
                paper,
                paper_revisions,
                revision_artifact_bindings,
                section_heads,
                section_revisions,
                section_merges,
            ) {
                blockers.push("paper_revision_section_lineage_required".to_string());
                next_actions.push("create_paper_revision".to_string());
            } else {
                next_actions.push("transition_paper_project".to_string());
            }
            (
                Some(PaperPhase::AuthorApproval),
                "confirm_reproduction_readiness",
            )
        }
        PaperPhase::AuthorApproval => {
            if paper.release_candidate_revision_id.is_none() {
                blockers.push("release_candidate_required".to_string());
                next_actions.push("promote_paper_release_candidate".to_string());
            }
            let consented_players = authorship_consents
                .iter()
                .map(|consent| consent.player_id)
                .collect::<HashSet<_>>();
            if consented_players.len() < team.members.len() {
                blockers.push("all_author_consents_required".to_string());
                next_actions.push("create_authorship_consent".to_string());
            }
            if blockers.is_empty() && joint_submission.is_none() {
                next_actions.push("finalize_joint_paper_submission".to_string());
            }
            // Submission-ready is reached only through signed finalization;
            // it is not a Paper phase-transition command target.
            (None, "collect_author_approval")
        }
        PaperPhase::IntegrityHold => {
            blockers.push("integrity_hold_open".to_string());
            next_actions.push("submit_appeal".to_string());
            (None, "resolve_integrity_hold")
        }
        PaperPhase::SubmissionReady => (None, "author_raid_complete"),
    };
    if paper
        .challenge_ruleset_snapshot
        .as_ref()
        .is_some_and(|snapshot| {
            snapshot.enforcement == crate::ChallengeRulesetEnforcementV1::AuthoritativeV1
        })
    {
        let facts = super::AuthorPhaseGateFacts {
            work_item: work_items
                .iter()
                .any(|item| item.status != WorkItemStatus::Cancelled),
            work_item_count: work_items
                .iter()
                .filter(|item| item.status != WorkItemStatus::Cancelled)
                .count() as u32,
            accepted_work_item_count: work_items
                .iter()
                .filter(|item| item.status == WorkItemStatus::Accepted)
                .count() as u32,
            all_work_items_terminal: !work_items.is_empty()
                && work_items.iter().all(|item| {
                    matches!(
                        item.status,
                        WorkItemStatus::Accepted | WorkItemStatus::Cancelled
                    )
                }),
            paper_revision: paper.current_revision_id.is_some() && !paper_revisions.is_empty(),
            paper_revision_count: paper_revisions.len() as u32,
            paper_revision_covers_section_merges: paper_revision_covers_section_merges(
                paper,
                paper_revisions,
                revision_artifact_bindings,
                section_heads,
                section_revisions,
                section_merges,
            ),
            collaboration: CollaborationPhaseGateFacts {
                artifact_manifest: !artifact_manifests.is_empty(),
                evidence_card: !evidence_cards.is_empty(),
                citation: !citations.is_empty(),
                experiment_plan: !experiment_plans.is_empty(),
                retained_run: !runs.is_empty(),
                claim: !claims.is_empty(),
                section_revision: !section_revisions.is_empty(),
                approving_section_review: section_reviews
                    .iter()
                    .any(|review| review.verdict == SectionReviewVerdict::Approve),
                section_merge: !section_merges.is_empty(),
            },
            collaboration_counts: CollaborationPhaseGateCounts {
                artifact_manifests: artifact_manifests.len() as u32,
                evidence_cards: evidence_cards.len() as u32,
                citations: citations.len() as u32,
                experiment_plans: experiment_plans.len() as u32,
                retained_runs: runs.len() as u32,
                successful_runs: runs
                    .iter()
                    .filter(|run| run.status == RunStatus::Succeeded)
                    .count() as u32,
                retained_failed_runs: runs
                    .iter()
                    .filter(|run| run.status == RunStatus::Failed && run.failure_hash.is_some())
                    .count() as u32,
                claims: claims.len() as u32,
                section_revisions: section_revisions.len() as u32,
                approving_section_reviews: section_reviews
                    .iter()
                    .filter(|review| review.verdict == SectionReviewVerdict::Approve)
                    .count() as u32,
                section_merges: section_merges.len() as u32,
            },
        };
        let projected_blockers = if let Some(next) = next_phase {
            Some(super::author_phase_gate_blockers(paper, next, facts))
        } else if paper.phase == PaperPhase::AuthorApproval {
            let consented_players = authorship_consents
                .iter()
                .map(|consent| consent.player_id)
                .collect::<HashSet<_>>();
            Some(super::challenge_victory_gate_blockers(
                paper,
                facts,
                paper.release_candidate_revision_id.is_some(),
                consented_players.len() == team.members.len(),
            ))
        } else {
            None
        };
        if let Some(projected_blockers) = projected_blockers {
            blockers = projected_blockers
                .unwrap_or_else(|_| vec!["challenge_ruleset_integrity_error".to_string()]);
            next_actions.clear();
            if blockers.is_empty() {
                if paper.phase == PaperPhase::AuthorApproval {
                    if joint_submission.is_none() {
                        next_actions.push("finalize_joint_paper_submission".to_string());
                    }
                } else {
                    next_actions.push("transition_paper_project".to_string());
                }
            } else {
                for blocker in &blockers {
                    let action = if blocker.contains("all_work_items_terminal") {
                        "transition_paper_work_item"
                    } else if blocker.contains("accepted_work_items")
                        || blocker.contains("work_item")
                    {
                        "create_paper_work_item"
                    } else if blocker.contains("release_candidate") {
                        "promote_paper_release_candidate"
                    } else if blocker.contains("all_author_consents") {
                        "create_authorship_consent"
                    } else if blocker.contains("artifact_manifest") {
                        "register_artifact"
                    } else if blocker.contains("evidence_card") {
                        "create_evidence_card"
                    } else if blocker.contains("citation") {
                        "create_citation_record"
                    } else if blocker.contains("experiment_plan") {
                        "create_experiment_plan"
                    } else if blocker.contains("run") {
                        "create_run_record"
                    } else if blocker.contains("claim") {
                        "create_claim_record"
                    } else if blocker.contains("section_revision") {
                        "create_section_revision"
                    } else if blocker.contains("section_review") {
                        "submit_review"
                    } else if blocker.contains("paper_revision") {
                        "create_paper_revision"
                    } else if blocker.contains("section_merge") {
                        "merge_section"
                    } else {
                        "inspect_challenge_ruleset"
                    };
                    if !next_actions.iter().any(|existing| existing == action) {
                        next_actions.push(action.to_string());
                    }
                }
            }
        }
    }
    if !matches!(
        paper.outcome,
        super::PaperChallengeOutcomeV1::InProgress
            | super::PaperChallengeOutcomeV1::SubmissionReady
    ) {
        next_phase = None;
        objective = "challenge_terminal";
        blockers = vec![format!(
            "paper_challenge_outcome_terminal:{}",
            paper.outcome.as_str()
        )];
        next_actions.clear();
    } else if automatic_challenge_expiry_due(paper, projection_now) {
        // The authenticated room/event read path materializes this state
        // before projecting the room. Keep this fail-closed fallback free of
        // a Captain-only action in case a caller projects an old snapshot.
        next_phase = None;
        objective = "challenge_expiry_materialization_pending";
        blockers = vec![if paper.active_rework_id.is_some() {
            AUTOMATIC_REWORK_EXPIRY_REASON.to_string()
        } else {
            AUTOMATIC_CHALLENGE_EXPIRY_REASON.to_string()
        }];
        next_actions.clear();
    }
    let actor_role = team
        .members
        .iter()
        .find(|member| member.player_id == actor_player_id)
        .map(|member| member.role.as_str())
        .unwrap_or("support");
    let role_enforced = canonical_author_roles_are_enforced(team);
    let (personal_objective, primary_actions): (&str, &[&str]) = match actor_role {
        "captain" => (
            "coordinate_team_and_phase_gate",
            &["create_paper_work_item", "transition_paper_project"],
        ),
        "evidence" => (
            "protect_claim_and_source_quality",
            &[
                "create_evidence_card",
                "create_citation_record",
                "create_claim_record",
            ],
        ),
        "experiment" => (
            "own_preregistration_and_reproducibility",
            &["create_experiment_plan", "create_run_record"],
        ),
        _ => ("support_current_phase", &[]),
    };
    let shared_actions = [
        "register_artifact",
        "create_figure_lineage",
        "create_paper_revision",
        "create_section_revision",
        "submit_review",
        "merge_section",
        "create_authorship_consent",
    ];
    let collaboration_overrides: &[&str] = if actor_role == "captain" {
        &[
            "assign_work_item_to_teammate",
            "transition_any_team_work_item",
        ]
    } else {
        &[
            "create_self_assigned_work_item",
            "transition_assigned_work_item",
        ]
    };
    let mut role_blockers = Vec::new();
    if role_enforced {
        for action in &next_actions {
            let required_role = match action.as_str() {
                "transition_paper_project" => Some("captain"),
                "create_evidence_card" | "create_citation_record" | "create_claim_record" => {
                    Some("evidence")
                }
                "create_experiment_plan" | "create_run_record" => Some("experiment"),
                _ => None,
            };
            if required_role.is_some_and(|required| required != actor_role) {
                role_blockers.push(format!(
                    "{}_role_required:{}",
                    required_role.expect("role is present"),
                    action
                ));
            }
        }
    }
    let role_resources = project_role_resources(
        paper,
        actor_role,
        evidence_cards.len(),
        experiment_plans.len(),
        projection_now,
    );
    AuthorRaidProgressV1 {
        schema: AUTHOR_RAID_PROGRESS_SCHEMA_V1.to_string(),
        phase: paper.phase,
        next_phase,
        player_phase: player_phase_semantic(paper.phase).to_string(),
        next_player_phase: next_phase.map(player_phase_semantic).map(str::to_string),
        objective: objective.to_string(),
        actor_role: actor_role.to_string(),
        role_enforcement: if role_enforced {
            "enforced"
        } else {
            "legacy_advisory"
        }
        .to_string(),
        personal_objective: personal_objective.to_string(),
        primary_actions: primary_actions
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
        shared_actions: shared_actions
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
        collaboration_overrides: collaboration_overrides
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
        role_blockers,
        role_resources,
        actor_can_transition: !role_enforced || actor_role == "captain",
        transition_ready: blockers.is_empty(),
        blockers,
        next_actions,
    }
}

fn player_phase_semantic(phase: PaperPhase) -> &'static str {
    match phase {
        PaperPhase::Reproducing => "reproduction_readiness",
        _ => phase.as_str(),
    }
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
        .route(
            "/v2/hepta/matchmaking/tickets/:ticket_id/cancel",
            post(cancel_matchmaking_ticket),
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
        .merge(role_resources_v1::router())
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
    SectionMerge,
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
            Self::SectionMerge => "section_merge",
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
            Self::SectionMerge => {
                matches!(phase, PaperPhase::Drafting | PaperPhase::IntegrityReview)
            }
        }
    }
}

fn require_collaboration_phase(
    paper: &PaperProject,
    mutation: CollaborationMutation,
) -> Result<(), ApiError> {
    require_collaboration_phase_at(paper, mutation, Utc::now())
}

fn require_collaboration_phase_at(
    paper: &PaperProject,
    mutation: CollaborationMutation,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    super::ensure_paper_gameplay_active(paper, now)?;
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

fn automatic_challenge_expiry_due(paper: &PaperProject, now: DateTime<Utc>) -> bool {
    if paper.outcome != PaperChallengeOutcomeV1::InProgress {
        return false;
    }
    if paper.active_rework_id.is_some()
        && paper.active_rework_cycle.is_some()
        && paper.rework_expires_at.is_some()
    {
        return paper
            .rework_expires_at
            .is_some_and(|expires_at| now >= expires_at);
    }
    paper.active_rework_id.is_none()
        && paper.active_rework_cycle.is_none()
        && paper.rework_expires_at.is_none()
        && paper
            .grace_expires_at
            .is_some_and(|grace_expires_at| now >= grace_expires_at)
}

fn apply_automatic_challenge_expiry(
    paper: &mut PaperProject,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    if !automatic_challenge_expiry_due(paper, now) {
        return Ok(false);
    }
    let rework_expiry = paper.active_rework_id.is_some();
    let expires_at = if rework_expiry {
        paper
            .rework_expires_at
            .expect("automatic rework expiry due implies an immutable lease deadline")
    } else {
        paper
            .grace_expires_at
            .expect("automatic expiry due implies an immutable grace deadline")
    };
    paper.outcome = PaperChallengeOutcomeV1::Expired;
    paper.outcome_reason = Some(
        if rework_expiry {
            AUTOMATIC_REWORK_EXPIRY_REASON
        } else {
            AUTOMATIC_CHALLENGE_EXPIRY_REASON
        }
        .to_string(),
    );
    // The gameplay terminal fact is the immutable grace boundary. `updated_at`
    // and the emitted event timestamp below record when lazy materialization
    // actually happened, so polling frequency cannot change the terminal fact.
    paper.terminal_at = Some(expires_at);
    paper.active_rework_id = None;
    paper.active_rework_cycle = None;
    paper.rework_expires_at = None;
    paper.version = paper
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("automatic Paper expiry version overflow"))?;
    paper.updated_at = now;
    Ok(true)
}

fn automatic_challenge_expiry_idempotency_key(paper: &PaperProject) -> String {
    match paper.active_rework_id {
        Some(rework_id) => format!(
            "automatic-rework-expiry:{}:{rework_id}",
            paper.paper_project_id
        ),
        None => format!("automatic-challenge-expiry:{}", paper.paper_project_id),
    }
}

fn automatic_challenge_expiry_payload(paper: &PaperProject) -> Value {
    json!({
        "paper_project_id": paper.paper_project_id,
        "outcome": paper.outcome,
        "reason_code": paper.outcome_reason,
        "terminal_at": paper.terminal_at,
        "automatic": true,
    })
}

fn materialize_automatic_challenge_expiry_memory(
    memory: &mut PaperRaidMemory,
    paper_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<PaperProject>, ApiError> {
    let Some(paper) = memory.papers.get_mut(&paper_id) else {
        return Ok(None);
    };
    let idempotency_key = automatic_challenge_expiry_idempotency_key(paper);
    if !apply_automatic_challenge_expiry(paper, now)? {
        return Ok(None);
    }
    let response = paper.clone();
    push_room_event_memory(
        memory,
        AUTOMATIC_CHALLENGE_EXPIRY_OPERATION,
        &idempotency_key,
        AUTOMATIC_CHALLENGE_EXPIRY_EVENT,
        paper_id,
        paper_id,
        response.version,
        automatic_challenge_expiry_payload(&response),
    );
    Ok(Some(response))
}

async fn materialize_automatic_challenge_expiry_postgres(
    tx: &mut Transaction<'_, Postgres>,
    mut paper: PaperProject,
    now: DateTime<Utc>,
) -> Result<Option<PaperProject>, ApiError> {
    let idempotency_key = automatic_challenge_expiry_idempotency_key(&paper);
    if !apply_automatic_challenge_expiry(&mut paper, now)? {
        return Ok(None);
    }
    let previous_version = paper
        .version
        .checked_sub(1)
        .ok_or_else(|| ApiError::internal("automatic Paper expiry version underflow"))?;
    let record_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode expired paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set outcome='expired',outcome_reason=$1,terminal_at=$2,version=$3,
             active_rework_id=null,active_rework_cycle=null,rework_expires_at=null,
             record_json=$4::jsonb,updated_at=$5
         where paper_project_id=$6 and outcome='in_progress' and version=$7",
    )
    .bind(&paper.outcome_reason)
    .bind(paper.terminal_at)
    .bind(
        i64::try_from(paper.version)
            .map_err(|_| ApiError::internal("automatic Paper expiry version overflow"))?,
    )
    .bind(record_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(
        i64::try_from(previous_version)
            .map_err(|_| ApiError::internal("automatic Paper expiry version overflow"))?,
    )
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "automatic_paper_expiry_race",
            "paper project changed while its challenge expiry was materialized",
        ));
    }
    insert_room_event_postgres(
        tx,
        AUTOMATIC_CHALLENGE_EXPIRY_OPERATION,
        &idempotency_key,
        AUTOMATIC_CHALLENGE_EXPIRY_EVENT,
        paper.paper_project_id,
        paper.paper_project_id,
        paper.version,
        automatic_challenge_expiry_payload(&paper),
    )
    .await?;
    Ok(Some(paper))
}

/// Lazily materialize the immutable deadline outcome on the first authorized
/// Paper read at or after the half-open grace boundary. Memory serializes on
/// the Paper-Raid write lock; PostgreSQL serializes on the Paper row. Both
/// paths re-check the outcome after acquiring their lock, so concurrent reads
/// and retries produce one aggregate transition and one event.
pub(super) async fn ensure_automatic_challenge_expiry_materialized(
    state: &AppState,
    paper_id: Uuid,
    assertion: &ConsumerUserAssertionClaimV2,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if state.pool.is_none() {
        {
            let memory = state.paper_raid.read().await;
            let Some(paper) = memory.papers.get(&paper_id) else {
                return Ok(());
            };
            if !automatic_challenge_expiry_due(paper, now) {
                return Ok(());
            }
        }
        let mut memory = state.paper_raid.write().await;
        let (paper, _) = paper_and_team_memory(&memory, paper_id, assertion)?;
        if !automatic_challenge_expiry_due(&paper, now) {
            return Ok(());
        }
        ensure_paper_finality_v2_source_unsealed_memory(state, paper_id).await?;
        let _ = materialize_automatic_challenge_expiry_memory(&mut memory, paper_id, now)?;
        return Ok(());
    }

    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state unavailable"))?;
    let candidate = sqlx::query(
        "select record_json,clock_timestamp() as storage_now
         from hepta_paper_projects where paper_project_id=$1",
    )
    .bind(paper_id)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::database)?;
    let Some(candidate) = candidate else {
        return Ok(());
    };
    let storage_now: DateTime<Utc> = candidate.get("storage_now");
    let candidate: PaperProject = decode_record(candidate.get("record_json"), "paper project")?;
    if !automatic_challenge_expiry_due(&candidate, storage_now) {
        return Ok(());
    }

    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    assert_team_actor_postgres(&mut tx, candidate.team_id, assertion).await?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let row = sqlx::query("select record_json from hepta_paper_projects where paper_project_id=$1")
        .bind(paper_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let paper: PaperProject = decode_record(row.get("record_json"), "paper project")?;
    let storage_now = postgres_transaction_now(&mut tx).await?;
    let _ = materialize_automatic_challenge_expiry_postgres(&mut tx, paper, storage_now).await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(())
}

async fn ensure_player_automatic_challenge_expiries_materialized(
    state: &AppState,
    assertion: &ConsumerUserAssertionClaimV2,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let due_paper_ids = if let Some(pool) = &state.pool {
        sqlx::query(
            "select p.paper_project_id
             from hepta_paper_projects p
             join hepta_research_team_members m on m.team_id=p.team_id
             where m.player_id=$1 and p.outcome='in_progress'
               and (
                 (p.active_rework_id is not null and p.active_rework_cycle is not null
                    and p.rework_expires_at is not null and p.rework_expires_at <= $2)
                 or
                 (p.active_rework_id is null and p.active_rework_cycle is null
                    and p.rework_expires_at is null and p.grace_expires_at is not null
                    and p.grace_expires_at <= $2)
               )
             order by coalesce(p.rework_expires_at,p.grace_expires_at),p.paper_project_id",
        )
        .bind(assertion.player_id)
        .bind(now)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| row.get("paper_project_id"))
        .collect::<Vec<Uuid>>()
    } else {
        let memory = state.paper_raid.read().await;
        let team_ids = memory
            .teams
            .values()
            .filter(|team| {
                team.members
                    .iter()
                    .any(|member| member.player_id == assertion.player_id)
            })
            .map(|team| team.team_id)
            .collect::<HashSet<_>>();
        let mut paper_ids = memory
            .papers
            .values()
            .filter(|paper| {
                team_ids.contains(&paper.team_id) && automatic_challenge_expiry_due(paper, now)
            })
            .map(|paper| paper.paper_project_id)
            .collect::<Vec<_>>();
        paper_ids.sort_unstable();
        paper_ids
    };
    for paper_id in due_paper_ids {
        ensure_automatic_challenge_expiry_materialized(state, paper_id, assertion, now).await?;
    }
    Ok(())
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
    if let Some(party_code_hash) = request.party_code_hash.as_deref() {
        validate_digest_v2("party_code_hash", party_code_hash)?;
    }
    if request.requested_team_size != 3 {
        return Err(ApiError::bad_request(
            "alpha_team_size_requires_three",
            "the v0 alpha matcher supports exactly three players; Team and protocol aggregates remain 3-5",
        ));
    }
    if request.roles.is_empty() || request.roles.len() > 3 {
        return Err(ApiError::bad_request(
            "invalid_matchmaking_roles",
            "matchmaking ticket requires 1-3 canonical distinct roles",
        ));
    }
    let mut roles = HashSet::new();
    for role in &request.roles {
        validate_collaboration_text("role", role)?;
        if !matches!(role.as_str(), "captain" | "evidence" | "experiment") {
            return Err(ApiError::bad_request(
                "invalid_matchmaking_role",
                "matchmaking roles are restricted to captain, evidence, and experiment",
            ));
        }
        if !roles.insert(role) {
            return Err(ApiError::bad_request(
                "duplicate_matchmaking_role",
                "matchmaking roles must be unique",
            ));
        }
    }
    Ok(())
}

fn distinct_role_assignment(tickets: &[MatchmakingTicket]) -> Option<Vec<String>> {
    fn assign(
        tickets: &[MatchmakingTicket],
        index: usize,
        used: &mut HashSet<String>,
        roles: &mut Vec<String>,
    ) -> bool {
        if index == tickets.len() {
            return true;
        }
        for role in &tickets[index].roles {
            if canonical_matchmaking_role_bit(role).is_none() {
                continue;
            }
            if used.insert(role.clone()) {
                roles.push(role.clone());
                if assign(tickets, index + 1, used, roles) {
                    return true;
                }
                roles.pop();
                used.remove(role);
            }
        }
        false
    }

    let mut used = HashSet::new();
    let mut roles = Vec::with_capacity(tickets.len());
    assign(tickets, 0, &mut used, &mut roles).then_some(roles)
}

fn validate_premade_party_admission(
    existing: &[MatchmakingTicket],
    candidate: &MatchmakingTicket,
) -> Result<(), ApiError> {
    let Some(party_code_hash) = candidate.party_code_hash.as_deref() else {
        return Ok(());
    };
    if live_party_ticket_count(existing.iter(), candidate.challenge_id, party_code_hash) >= 3 {
        return Err(ApiError::conflict(
            "party_queue_full",
            "the premade party already has three live matchmaking tickets",
        ));
    }
    let mut party = existing
        .iter()
        .filter(|ticket| {
            ticket.challenge_id == candidate.challenge_id
                && ticket.party_code_hash.as_deref() == Some(party_code_hash)
                && matches!(
                    ticket.status,
                    MatchmakingTicketStatus::Queued | MatchmakingTicketStatus::Matched
                )
        })
        .cloned()
        .collect::<Vec<_>>();
    party.push(candidate.clone());
    if party
        .iter()
        .any(|ticket| !matchmaking_partition_compatible(candidate, ticket))
    {
        return Err(ApiError::conflict(
            "party_availability_conflict",
            "premade party members must choose one exact availability window; a queued member can cancel and rejoin with the shared window",
        ));
    }
    if distinct_role_assignment(&party).is_none() {
        return Err(ApiError::conflict(
            "party_role_conflict",
            "premade party role preferences cannot cover distinct Captain, Evidence, and Experiment slots; a queued member can cancel and rejoin with compatible roles",
        ));
    }
    Ok(())
}

fn canonical_matchmaking_role_bit(role: &str) -> Option<u8> {
    match role {
        "captain" => Some(0b001),
        "evidence" => Some(0b010),
        "experiment" => Some(0b100),
        _ => None,
    }
}

fn maximum_assignable_role_slots_for(
    ticket: &MatchmakingTicket,
    compatible: &[MatchmakingTicket],
) -> u32 {
    let mut reachable = [false; 8];
    for role in &ticket.roles {
        if let Some(bit) = canonical_matchmaking_role_bit(role) {
            reachable[usize::from(bit)] = true;
        }
    }
    let mut seen_players = HashSet::from([ticket.player_id]);
    for candidate in compatible {
        if !matchmaking_partition_compatible(ticket, candidate)
            || !seen_players.insert(candidate.player_id)
        {
            continue;
        }
        let previous = reachable;
        for (mask, is_reachable) in previous.into_iter().enumerate() {
            if !is_reachable {
                continue;
            }
            for role in &candidate.roles {
                let Some(bit) = canonical_matchmaking_role_bit(role) else {
                    continue;
                };
                if mask & usize::from(bit) == 0 {
                    reachable[mask | usize::from(bit)] = true;
                }
            }
        }
    }
    reachable
        .iter()
        .enumerate()
        .filter(|(_, reachable)| **reachable)
        .map(|(mask, _)| mask.count_ones())
        .max()
        .unwrap_or_default()
}

fn matchmaking_ticket_deadline(ticket: &MatchmakingTicket) -> DateTime<Utc> {
    ticket.expires_at.unwrap_or_else(|| {
        ticket.created_at + chrono::Duration::seconds(MATCHMAKING_TICKET_TTL_SECONDS)
    })
}

fn matchmaking_ticket_is_live_at(ticket: &MatchmakingTicket, now: DateTime<Utc>) -> bool {
    ticket.status == MatchmakingTicketStatus::Queued && matchmaking_ticket_deadline(ticket) > now
}

fn expire_matchmaking_ticket(
    ticket: &mut MatchmakingTicket,
    now: DateTime<Utc>,
    player_eligible: bool,
) -> bool {
    if ticket.status == MatchmakingTicketStatus::Queued
        && (!player_eligible || matchmaking_ticket_deadline(ticket) <= now)
    {
        ticket.status = MatchmakingTicketStatus::Expired;
        ticket.matched_proposal_id = None;
        ticket.expires_at = Some(if player_eligible {
            matchmaking_ticket_deadline(ticket)
        } else {
            now
        });
        ticket.queue_hint = None;
        ticket.version += 1;
        ticket.updated_at = now;
        return true;
    }
    false
}

fn consume_materialized_ticket(
    ticket: &mut MatchmakingTicket,
    proposal_id: Uuid,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    if ticket.matched_proposal_id != Some(proposal_id) {
        return Err(ApiError::internal(
            "materialized team source ticket points at a different proposal",
        ));
    }
    match ticket.status {
        MatchmakingTicketStatus::Matched => {
            ticket.status = MatchmakingTicketStatus::Consumed;
            ticket.queue_hint = None;
            ticket.version += 1;
            ticket.updated_at = now;
            Ok(true)
        }
        MatchmakingTicketStatus::Consumed => Ok(false),
        _ => Err(ApiError::internal(
            "materialized team source ticket is neither matched nor consumed",
        )),
    }
}

fn project_matchmaking_ticket(
    mut ticket: MatchmakingTicket,
    queue: &[MatchmakingTicket],
    now: DateTime<Utc>,
) -> MatchmakingTicket {
    let deadline = matchmaking_ticket_deadline(&ticket);
    ticket.expires_at = Some(deadline);
    if ticket.status == MatchmakingTicketStatus::Queued && deadline <= now {
        ticket.status = MatchmakingTicketStatus::Expired;
        ticket.matched_proposal_id = None;
    }

    let waited_seconds = now
        .signed_duration_since(ticket.created_at)
        .num_seconds()
        .max(0) as u64;
    let expires_in_seconds = deadline.signed_duration_since(now).num_seconds().max(0) as u64;
    // The authoritative matcher applies one bounded FIFO horizon across the
    // whole challenge before it partitions by availability and premade party.
    // Project the exact same horizon so `eta_seconds = 0` can never claim that
    // a tail ticket is immediately matchable when the matcher cannot inspect
    // it yet.
    let mut horizon = queue
        .iter()
        .filter(|candidate| {
            candidate.challenge_id == ticket.challenge_id
                && candidate.requested_team_size == 3
                && matchmaking_ticket_is_live_at(candidate, now)
        })
        .cloned()
        .collect::<Vec<_>>();
    horizon.sort_by_key(|candidate| (candidate.created_at, candidate.ticket_id));
    horizon.truncate(MAX_ALPHA_MATCH_CANDIDATES);
    let mut seen_players = HashSet::new();
    horizon.retain(|candidate| seen_players.insert(candidate.player_id));
    // `eta_seconds = 0` means this exact ticket belongs to the one triplet the
    // authoritative matcher would select next across every partition. A
    // merely-complete later partition must not look immediately ready while
    // an older compatible triplet wins the global FIFO comparison.
    let globally_selected = first_compatible_triplet(&horizon);
    let compatible = horizon
        .into_iter()
        .filter(|candidate| matchmaking_partition_compatible(&ticket, candidate))
        .collect::<Vec<_>>();
    let compatible_pool_size = u32::try_from(compatible.len()).unwrap_or(u32::MAX);
    let queue_position = compatible
        .iter()
        .position(|candidate| candidate.ticket_id == ticket.ticket_id)
        .and_then(|position| u32::try_from(position + 1).ok());
    let offered_roles = compatible
        .iter()
        .flat_map(|candidate| candidate.roles.iter().map(String::as_str))
        .collect::<HashSet<_>>();
    let missing_roles = ["captain", "evidence", "experiment"]
        .into_iter()
        .filter(|role| !offered_roles.contains(role))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let assignable_role_slots = maximum_assignable_role_slots_for(&ticket, &compatible);
    let compatible_players_needed = ticket
        .requested_team_size
        .saturating_sub(assignable_role_slots);
    let ready_now = ticket.status == MatchmakingTicketStatus::Queued
        && globally_selected.is_some_and(|selected| {
            selected
                .iter()
                .any(|item| item.ticket_id == ticket.ticket_id)
        });
    let (state, eta_seconds, message) = match ticket.status {
        MatchmakingTicketStatus::Queued if ready_now => ("ready", Some(0), "compatible_team_ready"),
        MatchmakingTicketStatus::Queued
            if ticket.party_code_hash.is_some()
                && compatible_pool_size < ticket.requested_team_size =>
        {
            ("waiting", None, "waiting_for_party_members")
        }
        MatchmakingTicketStatus::Queued if !missing_roles.is_empty() => {
            ("waiting", None, "waiting_for_required_roles")
        }
        MatchmakingTicketStatus::Queued if compatible_players_needed > 0 => {
            ("waiting", None, "waiting_for_role_distribution")
        }
        MatchmakingTicketStatus::Queued => ("waiting", None, "waiting_for_compatible_players"),
        MatchmakingTicketStatus::Matched => ("matched", Some(0), "team_proposal_created"),
        MatchmakingTicketStatus::Consumed => ("consumed", None, "team_materialized"),
        MatchmakingTicketStatus::Cancelled => ("cancelled", None, "ticket_cancelled"),
        MatchmakingTicketStatus::Expired => ("expired", None, "ticket_expired"),
    };
    ticket.queue_hint = Some(MatchmakingQueueHintV1 {
        schema: MATCHMAKING_QUEUE_HINT_SCHEMA_V1.to_string(),
        state: state.to_string(),
        queue_position,
        compatible_pool_size,
        compatible_players_needed,
        missing_roles,
        waited_seconds,
        expires_in_seconds,
        eta_seconds,
        message: message.to_string(),
    });
    ticket
}

fn first_compatible_triplet(tickets: &[MatchmakingTicket]) -> Option<Vec<MatchmakingTicket>> {
    const ROLE_ORDERS: [[&str; 3]; 6] = [
        ["captain", "evidence", "experiment"],
        ["captain", "experiment", "evidence"],
        ["evidence", "captain", "experiment"],
        ["evidence", "experiment", "captain"],
        ["experiment", "captain", "evidence"],
        ["experiment", "evidence", "captain"],
    ];

    // Tickets arrive in deterministic FIFO order. Keep one active entry per
    // player, then solve the three canonical role permutations in linear time
    // per availability bucket. This avoids the former unbounded O(n^3) queue
    // scan and never matches players across incompatible play windows.
    let mut seen_players = HashSet::new();
    let mut buckets: HashMap<_, Vec<(usize, &MatchmakingTicket)>> = HashMap::new();
    for (index, ticket) in tickets.iter().enumerate() {
        if !seen_players.insert(ticket.player_id) {
            continue;
        }
        buckets
            .entry(matchmaking_partition_key(ticket))
            .or_default()
            .push((index, ticket));
    }

    let mut best: Option<([usize; 3], Vec<MatchmakingTicket>)> = None;
    for bucket in buckets.values() {
        for roles in ROLE_ORDERS {
            let mut cursor = 0;
            let mut selected = Vec::with_capacity(3);
            let mut indices = [0; 3];
            for (slot, role) in roles.into_iter().enumerate() {
                let Some((offset, (index, ticket))) = bucket[cursor..]
                    .iter()
                    .enumerate()
                    .find(|(_, (_, ticket))| ticket.roles.iter().any(|value| value == role))
                else {
                    selected.clear();
                    break;
                };
                cursor += offset + 1;
                indices[slot] = *index;
                selected.push((*ticket).clone());
            }
            if selected.len() == 3
                && best
                    .as_ref()
                    .is_none_or(|(best_indices, _)| indices < *best_indices)
            {
                best = Some((indices, selected));
            }
        }
    }
    best.map(|(_, tickets)| tickets)
}

fn select_alpha_match_at(
    tickets: impl Iterator<Item = MatchmakingTicket>,
    now: DateTime<Utc>,
) -> Vec<MatchmakingTicket> {
    let mut tickets: Vec<_> = tickets
        .filter(|ticket| {
            matchmaking_ticket_is_live_at(ticket, now) && ticket.requested_team_size == 3
        })
        .collect();
    tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id));
    tickets.truncate(MAX_ALPHA_MATCH_CANDIDATES);
    first_compatible_triplet(&tickets).unwrap_or_default()
}

#[cfg(test)]
fn select_alpha_match(tickets: impl Iterator<Item = MatchmakingTicket>) -> Vec<MatchmakingTicket> {
    select_alpha_match_at(tickets, Utc::now())
}

fn expire_due_matchmaking_tickets_memory(
    memory: &mut CollaborationMemory,
    eligible_player_ids: &HashSet<Uuid>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Vec<MatchmakingTicket> {
    let mut expired = Vec::new();
    for ticket in memory
        .tickets
        .values_mut()
        .filter(|ticket| ticket.challenge_id == challenge_id)
    {
        let player_eligible = eligible_player_ids.contains(&ticket.player_id);
        if expire_matchmaking_ticket(ticket, now, player_eligible) {
            expired.push(ticket.clone());
        }
    }
    expired
}

async fn expire_due_matchmaking_tickets_postgres(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<MatchmakingTicket>, ApiError> {
    let rows = sqlx::query(
        "select ticket.version,ticket.record_json,
                exists (
                    select 1 from hepta_human_players player
                    where player.player_id=ticket.player_id and player.status='active'
                      and (select count(*) from hepta_agent_bindings binding
                           where binding.player_id=ticket.player_id
                             and binding.status='active') = 1
                ) as player_eligible
         from hepta_matchmaking_tickets ticket
         where ticket.challenge_id=$1 and ticket.status='queued'
         order by ticket.created_at,ticket.ticket_id for update of ticket",
    )
    .bind(challenge_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut expired = Vec::new();
    for row in rows {
        let mut ticket: MatchmakingTicket =
            decode_record(row.get("record_json"), "matchmaking ticket")?;
        let actual_version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
        if ticket.version != actual_version {
            return Err(ApiError::internal(
                "matchmaking ticket record/version projection diverged",
            ));
        }
        if !expire_matchmaking_ticket(&mut ticket, now, row.get("player_eligible")) {
            continue;
        }
        let updated = sqlx::query(
            "update hepta_matchmaking_tickets
             set status='expired',matched_proposal_id=null,version=$1,
                 record_json=$2::jsonb,updated_at=$3,expires_at=$4
             where ticket_id=$5 and status='queued' and version=$6",
        )
        .bind(
            i64::try_from(ticket.version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .bind(serde_json::to_value(&ticket).map_err(|error| {
            ApiError::internal(format!("encode expired matchmaking ticket: {error}"))
        })?)
        .bind(now)
        .bind(matchmaking_ticket_deadline(&ticket))
        .bind(ticket.ticket_id)
        .bind(
            i64::try_from(actual_version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "matchmaking_queue_changed",
                "matchmaking ticket changed while expiring the queue",
            ));
        }
        expired.push(ticket);
    }
    Ok(expired)
}

fn team_proposal_deadline(proposal: &TeamProposal) -> DateTime<Utc> {
    proposal.expires_at.unwrap_or_else(|| {
        proposal.created_at + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS)
    })
}

fn project_team_proposal_deadline(mut proposal: TeamProposal) -> TeamProposal {
    proposal.expires_at = Some(team_proposal_deadline(&proposal));
    proposal
}

fn accepted_team_proposal_players(
    decisions: impl Iterator<Item = TeamProposalDecision>,
    proposal_id: Uuid,
) -> HashSet<Uuid> {
    decisions
        .filter(|decision| {
            decision.proposal_id == proposal_id
                && decision.decision == TeamProposalDecisionKind::Accept
        })
        .map(|decision| decision.player_id)
        .collect()
}

fn matchmaking_eligible_player_ids(memory: &PaperRaidMemory) -> HashSet<Uuid> {
    memory
        .players
        .values()
        .filter(|player| {
            player.status == HumanPlayerStatus::Active
                && memory
                    .bindings
                    .values()
                    .filter(|binding| {
                        binding.player_id == player.player_id
                            && binding.status == AgentBindingStatus::Active
                    })
                    .count()
                    == 1
        })
        .map(|player| player.player_id)
        .collect()
}

fn expire_team_proposal_memory(
    memory: &mut CollaborationMemory,
    eligible_player_ids: &HashSet<Uuid>,
    proposal_id: Uuid,
    now: DateTime<Utc>,
) -> Result<bool, ApiError> {
    let Some(snapshot) = memory.team_proposals.get(&proposal_id).cloned() else {
        return Ok(false);
    };
    let invalid_member = snapshot
        .member_player_ids
        .iter()
        .any(|player_id| !eligible_player_ids.contains(player_id));
    if !matches!(
        snapshot.status,
        TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
    ) || (team_proposal_deadline(&snapshot) > now && !invalid_member)
    {
        return Ok(false);
    }
    let accepted = accepted_team_proposal_players(
        memory.team_proposal_decisions.values().cloned(),
        proposal_id,
    );
    for ticket_id in &snapshot.source_ticket_ids {
        let ticket = memory
            .tickets
            .get_mut(ticket_id)
            .ok_or_else(|| ApiError::internal("expired team proposal source ticket is missing"))?;
        if ticket.status != MatchmakingTicketStatus::Matched
            || ticket.matched_proposal_id != Some(proposal_id)
        {
            return Err(ApiError::internal(
                "expired team proposal source ticket projection diverged",
            ));
        }
        ticket.status = if accepted.contains(&ticket.player_id)
            && eligible_player_ids.contains(&ticket.player_id)
            && matchmaking_ticket_deadline(ticket) > now
        {
            MatchmakingTicketStatus::Queued
        } else {
            MatchmakingTicketStatus::Expired
        };
        ticket.matched_proposal_id = None;
        ticket.version += 1;
        ticket.updated_at = now;
        ticket.queue_hint = None;
    }
    let proposal = memory
        .team_proposals
        .get_mut(&proposal_id)
        .expect("proposal snapshot came from memory");
    proposal.status = TeamProposalStatus::Expired;
    proposal.expires_at = Some(if invalid_member {
        now.min(team_proposal_deadline(proposal))
    } else {
        team_proposal_deadline(proposal)
    });
    proposal.version += 1;
    proposal.updated_at = now;
    Ok(true)
}

fn invalidate_team_proposal_provenance_memory(
    memory: &mut CollaborationMemory,
    reserved_team_ids: &HashSet<Uuid>,
    eligible_player_ids: &HashSet<Uuid>,
    proposal_id: Uuid,
    now: DateTime<Utc>,
) -> Result<TeamProposalExpirySweep, ApiError> {
    let snapshot = memory
        .team_proposals
        .get(&proposal_id)
        .cloned()
        .ok_or_else(|| ApiError::internal("invalid team proposal disappeared"))?;
    if !matches!(
        snapshot.status,
        TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
    ) {
        return Ok(TeamProposalExpirySweep::default());
    }
    for ticket_id in &snapshot.source_ticket_ids {
        let Some(ticket) = memory.tickets.get_mut(ticket_id) else {
            continue;
        };
        if ticket.status != MatchmakingTicketStatus::Matched
            || ticket.matched_proposal_id != Some(proposal_id)
        {
            continue;
        }
        ticket.status = if eligible_player_ids.contains(&ticket.player_id)
            && matchmaking_ticket_deadline(ticket) > now
        {
            MatchmakingTicketStatus::Queued
        } else {
            MatchmakingTicketStatus::Expired
        };
        ticket.matched_proposal_id = None;
        ticket.version += 1;
        ticket.updated_at = now;
        ticket.queue_hint = None;
    }
    let proposal = memory
        .team_proposals
        .get_mut(&proposal_id)
        .expect("invalid proposal snapshot came from memory");
    proposal.status = TeamProposalStatus::Expired;
    proposal.expires_at = Some(now.min(team_proposal_deadline(proposal)));
    proposal.version += 1;
    proposal.updated_at = now;
    let expired = proposal.clone();
    let replacements = auto_match_all_queued_tickets_memory(
        memory,
        reserved_team_ids,
        eligible_player_ids,
        snapshot.challenge_id,
        now,
    )?;
    Ok(TeamProposalExpirySweep {
        expired: vec![expired],
        replacements,
    })
}

#[derive(Debug, Default)]
struct TeamProposalExpirySweep {
    expired: Vec<TeamProposal>,
    replacements: Vec<TeamProposal>,
}

fn expire_due_team_proposals_memory(
    memory: &mut CollaborationMemory,
    reserved_team_ids: &HashSet<Uuid>,
    eligible_player_ids: &HashSet<Uuid>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<TeamProposalExpirySweep, ApiError> {
    let due = memory
        .team_proposals
        .values()
        .filter(|proposal| {
            proposal.challenge_id == challenge_id
                && matches!(
                    proposal.status,
                    TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
                )
                && (team_proposal_deadline(proposal) <= now
                    || proposal
                        .member_player_ids
                        .iter()
                        .any(|player_id| !eligible_player_ids.contains(player_id)))
        })
        .map(|proposal| proposal.proposal_id)
        .collect::<Vec<_>>();
    let mut expired = Vec::new();
    for proposal_id in due {
        if expire_team_proposal_memory(memory, eligible_player_ids, proposal_id, now)? {
            expired.push(
                memory
                    .team_proposals
                    .get(&proposal_id)
                    .expect("expired proposal remains stored")
                    .clone(),
            );
        }
    }
    let replacements = if expired.is_empty() {
        Vec::new()
    } else {
        auto_match_all_queued_tickets_memory(
            memory,
            reserved_team_ids,
            eligible_player_ids,
            challenge_id,
            now,
        )?
    };
    Ok(TeamProposalExpirySweep {
        expired,
        replacements,
    })
}

fn push_team_proposal_expiry_sweep_memory(
    memory: &mut PaperRaidMemory,
    sweep: &TeamProposalExpirySweep,
) -> Result<(), ApiError> {
    for proposal in &sweep.expired {
        push_memory_event(
            memory,
            "expire_team_proposal_v1",
            &proposal.proposal_id.to_string(),
            "hepta.paper_raid.team_proposal.expired.v1",
            proposal.proposal_id,
            proposal.version,
            json!({
                "proposal_id": proposal.proposal_id,
                "challenge_id": proposal.challenge_id,
                "expires_at": proposal.expires_at,
            }),
        )?;
    }
    push_automatic_team_proposal_events_memory(memory, &sweep.replacements)
}

fn push_automatic_team_proposal_events_memory(
    memory: &mut PaperRaidMemory,
    proposals: &[TeamProposal],
) -> Result<(), ApiError> {
    for proposal in proposals {
        push_memory_event(
            memory,
            "auto_match_team_proposal_v1",
            &proposal.proposal_id.to_string(),
            "hepta.paper_raid.team_proposal.created.v1",
            proposal.proposal_id,
            proposal.version,
            json!({
                "proposal_id": proposal.proposal_id,
                "challenge_id": proposal.challenge_id,
                "member_player_ids": &proposal.member_player_ids,
                "automatic_rematch": true,
            }),
        )?;
    }
    Ok(())
}

async fn expire_due_team_proposals_postgres(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<TeamProposal>, ApiError> {
    let rows = sqlx::query(
        "select proposal_id,version,record_json from hepta_team_proposals
         where challenge_id=$1 and status in ('proposed','accepted')
         order by expires_at,proposal_id for update",
    )
    .bind(challenge_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let mut expired = Vec::new();
    for row in rows {
        let proposal_id = row.get::<Uuid, _>("proposal_id");
        let actual_version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("team proposal version is invalid"))?;
        let mut proposal: TeamProposal = decode_record(row.get("record_json"), "team proposal")?;
        if proposal.version != actual_version
            || !matches!(
                proposal.status,
                TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
            )
        {
            return Err(ApiError::internal(
                "team proposal active-state projection diverged",
            ));
        }
        let accepted = sqlx::query(
            "select player_id from hepta_team_proposal_decisions
             where proposal_id=$1 and decision='accept'",
        )
        .bind(proposal_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| row.get::<Uuid, _>("player_id"))
        .collect::<HashSet<_>>();
        let mut accepted_player_ids = accepted.iter().copied().collect::<Vec<_>>();
        accepted_player_ids.sort_unstable();
        let ticket_rows = sqlx::query(
            "select ticket.version,ticket.record_json,player.status as player_status,
                    (select count(*) from hepta_agent_bindings binding
                     where binding.player_id=ticket.player_id and binding.status='active')
                        as active_binding_count
             from hepta_matchmaking_tickets ticket
             join hepta_human_players player on player.player_id=ticket.player_id
             where ticket.ticket_id=any($1) order by ticket.ticket_id
             for update of ticket for key share of player",
        )
        .bind(&proposal.source_ticket_ids)
        .fetch_all(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if ticket_rows.len() != proposal.source_ticket_ids.len() {
            return Err(ApiError::internal(
                "expired team proposal source ticket set is incomplete",
            ));
        }
        let invalid_member = ticket_rows.iter().any(|ticket_row| {
            ticket_row.get::<String, _>("player_status") != "active"
                || ticket_row.get::<i64, _>("active_binding_count") != 1
        });
        if team_proposal_deadline(&proposal) > now && !invalid_member {
            continue;
        }
        for ticket_row in ticket_rows {
            let actual_ticket_version = u64::try_from(ticket_row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
            let mut ticket: MatchmakingTicket =
                decode_record(ticket_row.get("record_json"), "matchmaking ticket")?;
            if ticket.version != actual_ticket_version
                || ticket.status != MatchmakingTicketStatus::Matched
                || ticket.matched_proposal_id != Some(proposal_id)
            {
                return Err(ApiError::internal(
                    "expired team proposal source ticket projection diverged",
                ));
            }
            ticket.status = if accepted.contains(&ticket.player_id)
                && ticket_row.get::<String, _>("player_status") == "active"
                && ticket_row.get::<i64, _>("active_binding_count") == 1
                && matchmaking_ticket_deadline(&ticket) > now
            {
                MatchmakingTicketStatus::Queued
            } else {
                MatchmakingTicketStatus::Expired
            };
            ticket.matched_proposal_id = None;
            ticket.version += 1;
            ticket.updated_at = now;
            ticket.queue_hint = None;
            let updated = sqlx::query(
                "update hepta_matchmaking_tickets
                 set status=$1,matched_proposal_id=null,version=$2,
                     record_json=$3::jsonb,updated_at=$4
                 where ticket_id=$5 and status='matched' and matched_proposal_id=$6
                   and version=$7",
            )
            .bind(ticket.status.as_str())
            .bind(
                i64::try_from(ticket.version)
                    .map_err(|_| ApiError::internal("ticket version overflow"))?,
            )
            .bind(serde_json::to_value(&ticket).map_err(|error| {
                ApiError::internal(format!("encode proposal-expired ticket: {error}"))
            })?)
            .bind(now)
            .bind(ticket.ticket_id)
            .bind(proposal_id)
            .bind(
                i64::try_from(actual_ticket_version)
                    .map_err(|_| ApiError::internal("ticket version overflow"))?,
            )
            .execute(&mut **tx)
            .await
            .map_err(ApiError::database)?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "team_proposal_expiry_race",
                    "matched ticket changed while its team proposal expired",
                ));
            }
        }
        proposal.status = TeamProposalStatus::Expired;
        proposal.expires_at = Some(if invalid_member {
            now.min(team_proposal_deadline(&proposal))
        } else {
            team_proposal_deadline(&proposal)
        });
        proposal.version += 1;
        proposal.updated_at = now;
        let updated = sqlx::query(
            "update hepta_team_proposals
             set status='expired',version=$1,record_json=$2::jsonb,updated_at=$3,expires_at=$4
             where proposal_id=$5 and status in ('proposed','accepted') and version=$6",
        )
        .bind(
            i64::try_from(proposal.version)
                .map_err(|_| ApiError::internal("proposal version overflow"))?,
        )
        .bind(serde_json::to_value(&proposal).map_err(|error| {
            ApiError::internal(format!("encode expired team proposal: {error}"))
        })?)
        .bind(now)
        .bind(team_proposal_deadline(&proposal))
        .bind(proposal_id)
        .bind(
            i64::try_from(actual_version)
                .map_err(|_| ApiError::internal("proposal version overflow"))?,
        )
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "team_proposal_expiry_race",
                "team proposal changed while its deadline was enforced",
            ));
        }
        insert_postgres_event(
            tx,
            "expire_team_proposal_v1",
            &proposal_id.to_string(),
            "hepta.paper_raid.team_proposal.expired.v1",
            proposal_id,
            proposal.version,
            json!({
                "proposal_id": proposal_id,
                "challenge_id": proposal.challenge_id,
                "accepted_player_ids": accepted_player_ids,
                "expires_at": proposal.expires_at,
            }),
        )
        .await?;
        expired.push(proposal);
    }
    if !expired.is_empty() {
        auto_match_all_queued_tickets_postgres(tx, challenge_id, now).await?;
    }
    Ok(expired)
}

async fn invalidate_team_proposal_provenance_postgres(
    tx: &mut Transaction<'_, Postgres>,
    proposal: &TeamProposal,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if !matches!(
        proposal.status,
        TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
    ) {
        return Ok(());
    }
    let rows = sqlx::query(
        "select ticket.version,ticket.record_json,player.status as player_status,
                (select count(*) from hepta_agent_bindings binding
                 where binding.player_id=ticket.player_id and binding.status='active')
                    as active_binding_count
         from hepta_matchmaking_tickets ticket
         left join hepta_human_players player on player.player_id=ticket.player_id
         where ticket.ticket_id=any($1) order by ticket.ticket_id
         for update of ticket",
    )
    .bind(&proposal.source_ticket_ids)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    for row in rows {
        let actual_version = u64::try_from(row.get::<i64, _>("version"))
            .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
        let mut ticket: MatchmakingTicket =
            decode_record(row.get("record_json"), "matchmaking ticket")?;
        if ticket.version != actual_version {
            return Err(ApiError::internal(
                "matchmaking ticket record/version projection diverged",
            ));
        }
        if ticket.status != MatchmakingTicketStatus::Matched
            || ticket.matched_proposal_id != Some(proposal.proposal_id)
        {
            continue;
        }
        let player_status: Option<String> = row.try_get("player_status").unwrap_or(None);
        ticket.status = if player_status.as_deref() == Some("active")
            && row.get::<i64, _>("active_binding_count") == 1
            && matchmaking_ticket_deadline(&ticket) > now
        {
            MatchmakingTicketStatus::Queued
        } else {
            MatchmakingTicketStatus::Expired
        };
        ticket.matched_proposal_id = None;
        ticket.version += 1;
        ticket.updated_at = now;
        ticket.queue_hint = None;
        let updated = sqlx::query(
            "update hepta_matchmaking_tickets
             set status=$1,matched_proposal_id=null,version=$2,
                 record_json=$3::jsonb,updated_at=$4
             where ticket_id=$5 and status='matched' and matched_proposal_id=$6
               and version=$7",
        )
        .bind(ticket.status.as_str())
        .bind(
            i64::try_from(ticket.version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .bind(serde_json::to_value(&ticket).map_err(|error| {
            ApiError::internal(format!("encode provenance-invalid ticket: {error}"))
        })?)
        .bind(now)
        .bind(ticket.ticket_id)
        .bind(proposal.proposal_id)
        .bind(
            i64::try_from(actual_version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "team_proposal_expiry_race",
                "matched ticket changed while invalid proposal provenance was withdrawn",
            ));
        }
    }
    let mut expired = proposal.clone();
    let previous_version = expired.version;
    expired.status = TeamProposalStatus::Expired;
    expired.expires_at = Some(now.min(team_proposal_deadline(&expired)));
    expired.version += 1;
    expired.updated_at = now;
    let updated = sqlx::query(
        "update hepta_team_proposals
         set status='expired',version=$1,record_json=$2::jsonb,updated_at=$3,expires_at=$4
         where proposal_id=$5 and status in ('proposed','accepted') and version=$6",
    )
    .bind(
        i64::try_from(expired.version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .bind(serde_json::to_value(&expired).map_err(|error| {
        ApiError::internal(format!("encode provenance-invalid proposal: {error}"))
    })?)
    .bind(now)
    .bind(team_proposal_deadline(&expired))
    .bind(expired.proposal_id)
    .bind(
        i64::try_from(previous_version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "team_proposal_expiry_race",
            "team proposal changed while invalid provenance was withdrawn",
        ));
    }
    insert_postgres_event(
        tx,
        "invalidate_team_proposal_provenance_v2",
        &expired.proposal_id.to_string(),
        "hepta.paper_raid.team_proposal.expired.v1",
        expired.proposal_id,
        expired.version,
        json!({
            "proposal_id": expired.proposal_id,
            "challenge_id": expired.challenge_id,
            "expires_at": expired.expires_at,
            "reason": "matcher_v2_provenance_invalid",
        }),
    )
    .await?;
    auto_match_all_queued_tickets_postgres(tx, expired.challenge_id, now).await?;
    Ok(())
}

fn deterministic_match_key_for_ticket_epochs(
    challenge_id: Uuid,
    ticket_epochs: &[(&MatchmakingTicket, u64)],
    partition: &MatchmakingTicket,
    assigned_roles: &[String],
) -> String {
    crate::paper_raid_contracts::sha256_digest(
        format!(
            "v2:{}:{}:{}:{}:{}",
            MATCHMAKING_SOLVER_VERSION_V2,
            challenge_id,
            partition.availability_hash,
            partition.party_code_hash.as_deref().unwrap_or("public"),
            ticket_epochs
                .iter()
                .zip(assigned_roles)
                .map(|((ticket, version), assigned_role)| {
                    format!(
                        "{}@{}@{}@{}@{}",
                        ticket.ticket_id,
                        version,
                        ticket.player_id,
                        ticket.roles.join(","),
                        assigned_role,
                    )
                })
                .collect::<Vec<_>>()
                .join(":")
        )
        .as_bytes(),
    )
}

fn frozen_team_proposal_preferences(
    tickets: &[MatchmakingTicket],
) -> Vec<TeamProposalSourcePreferenceV2> {
    tickets
        .iter()
        .map(|ticket| TeamProposalSourcePreferenceV2 {
            ticket_id: ticket.ticket_id,
            player_id: ticket.player_id,
            roles: ticket.roles.clone(),
            availability_hash: ticket.availability_hash.clone(),
            private_party: ticket.party_code_hash.is_some(),
        })
        .collect()
}

fn frozen_team_proposal_role_assignments(
    tickets: &[MatchmakingTicket],
    assigned_roles: &[String],
) -> Vec<TeamProposalRoleAssignmentV2> {
    tickets
        .iter()
        .zip(assigned_roles)
        .map(|(ticket, assigned_role)| TeamProposalRoleAssignmentV2 {
            ticket_id: ticket.ticket_id,
            player_id: ticket.player_id,
            assigned_role: assigned_role.clone(),
        })
        .collect()
}

fn build_team_proposal(tickets: &[MatchmakingTicket]) -> Result<TeamProposal, ApiError> {
    if tickets.len() != 3
        || tickets.iter().any(|ticket| {
            ticket.challenge_id != tickets[0].challenge_id
                || !matchmaking_partition_compatible(&tickets[0], ticket)
        })
    {
        return Err(ApiError::internal(
            "deterministic alpha matcher received a cross-partition or role-incompatible queue slice",
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
    let assigned_roles = distinct_role_assignment(tickets).ok_or_else(|| {
        ApiError::internal("deterministic alpha matcher received a role-incompatible queue slice")
    })?;
    let match_key = deterministic_match_key_for_ticket_epochs(
        tickets[0].challenge_id,
        &tickets
            .iter()
            .map(|ticket| (ticket, ticket.version))
            .collect::<Vec<_>>(),
        &tickets[0],
        &assigned_roles,
    );
    let now = Utc::now();
    Ok(TeamProposal {
        proposal_id: deterministic_uuid(&match_key),
        challenge_id: tickets[0].challenge_id,
        requested_team_size: 3,
        deterministic_match_key: match_key,
        member_player_ids,
        source_ticket_ids,
        solver_version: Some(MATCHMAKING_SOLVER_VERSION_V2.to_string()),
        source_preferences: frozen_team_proposal_preferences(tickets),
        role_assignments: frozen_team_proposal_role_assignments(tickets, &assigned_roles),
        status: TeamProposalStatus::Proposed,
        expires_at: Some(now + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS)),
        version: 1,
        created_at: now,
        updated_at: now,
    })
}

fn auto_match_queued_tickets_memory(
    memory: &mut CollaborationMemory,
    reserved_team_ids: &HashSet<Uuid>,
    eligible_player_ids: &HashSet<Uuid>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<TeamProposal>, ApiError> {
    let selected = select_alpha_match_at(
        memory
            .tickets
            .values()
            .filter(|ticket| {
                ticket.challenge_id == challenge_id
                    && eligible_player_ids.contains(&ticket.player_id)
            })
            .cloned(),
        now,
    );
    if selected.len() != 3 {
        return Ok(None);
    }
    let mut proposal = build_team_proposal(&selected)?;
    if reserved_team_ids.contains(&proposal.proposal_id) {
        return Err(ApiError::conflict(
            "matchmaking_team_id_conflict",
            "deterministic proposal team_id is already occupied by a non-matchmaking team",
        ));
    }
    proposal.created_at = now;
    proposal.updated_at = now;
    proposal.expires_at = Some(now + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS));
    for selected_ticket in &selected {
        let stored = memory
            .tickets
            .get_mut(&selected_ticket.ticket_id)
            .ok_or_else(|| ApiError::internal("selected replacement ticket is missing"))?;
        if stored.status != MatchmakingTicketStatus::Queued
            || stored.version != selected_ticket.version
        {
            return Err(ApiError::conflict(
                "matchmaking_queue_changed",
                "matchmaking queue changed during automatic rematch",
            ));
        }
        stored.status = MatchmakingTicketStatus::Matched;
        stored.matched_proposal_id = Some(proposal.proposal_id);
        stored.version += 1;
        stored.updated_at = now;
        stored.queue_hint = None;
    }
    if memory
        .team_proposals
        .insert(proposal.proposal_id, proposal.clone())
        .is_some()
    {
        return Err(ApiError::internal(
            "automatic rematch proposal identity already exists",
        ));
    }
    Ok(Some(proposal))
}

fn auto_match_all_queued_tickets_memory(
    memory: &mut CollaborationMemory,
    reserved_team_ids: &HashSet<Uuid>,
    eligible_player_ids: &HashSet<Uuid>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<TeamProposal>, ApiError> {
    let mut proposals = Vec::new();
    while let Some(proposal) = auto_match_queued_tickets_memory(
        memory,
        reserved_team_ids,
        eligible_player_ids,
        challenge_id,
        now,
    )? {
        proposals.push(proposal);
    }
    Ok(proposals)
}

async fn auto_match_queued_tickets_postgres(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Option<TeamProposal>, ApiError> {
    let queue_rows = sqlx::query(
        "select ticket.version,ticket.record_json
         from hepta_matchmaking_tickets ticket
         join hepta_human_players player on player.player_id=ticket.player_id
         where ticket.challenge_id=$1 and ticket.requested_team_size=3
           and ticket.status='queued' and player.status='active'
           and (select count(*) from hepta_agent_bindings binding
                where binding.player_id=ticket.player_id and binding.status='active') = 1
         order by ticket.created_at,ticket.ticket_id limit 2048
         for update of ticket for key share of player",
    )
    .bind(challenge_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let queue = queue_rows
        .into_iter()
        .map(|row| {
            let version = u64::try_from(row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
            let ticket: MatchmakingTicket =
                decode_record(row.get("record_json"), "matchmaking ticket")?;
            if ticket.version != version {
                return Err(ApiError::internal(
                    "matchmaking ticket record/version projection diverged",
                ));
            }
            Ok(ticket)
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let selected = select_alpha_match_at(queue.into_iter(), now);
    if selected.len() != 3 {
        return Ok(None);
    }
    let mut proposal = build_team_proposal(&selected)?;
    proposal.created_at = now;
    proposal.updated_at = now;
    proposal.expires_at = Some(now + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS));
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("hepta-paper-raid-team-id:{}", proposal.proposal_id))
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
    if sqlx::query("select 1 from hepta_research_teams where team_id=$1 limit 1")
        .bind(proposal.proposal_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(ApiError::database)?
        .is_some()
    {
        return Err(ApiError::conflict(
            "matchmaking_team_id_conflict",
            "deterministic proposal team_id is already occupied by a non-matchmaking team",
        ));
    }
    sqlx::query(
        "insert into hepta_team_proposals (
            proposal_id,challenge_id,requested_team_size,deterministic_match_key,status,
            member_player_ids,source_ticket_ids,solver_version,source_preferences,
            role_assignments,version,record_json,created_at,updated_at,expires_at
         ) values ($1,$2,3,$3,'proposed',$4,$5,$6,$7::jsonb,$8::jsonb,1,$9::jsonb,$10,$10,$11)",
    )
    .bind(proposal.proposal_id)
    .bind(proposal.challenge_id)
    .bind(&proposal.deterministic_match_key)
    .bind(&proposal.member_player_ids)
    .bind(&proposal.source_ticket_ids)
    .bind(proposal.solver_version.as_deref())
    .bind(
        serde_json::to_value(&proposal.source_preferences).map_err(|error| {
            ApiError::internal(format!(
                "encode automatic rematch source preferences: {error}"
            ))
        })?,
    )
    .bind(
        serde_json::to_value(&proposal.role_assignments).map_err(|error| {
            ApiError::internal(format!(
                "encode automatic rematch role assignments: {error}"
            ))
        })?,
    )
    .bind(serde_json::to_value(&proposal).map_err(|error| {
        ApiError::internal(format!("encode automatic rematch proposal: {error}"))
    })?)
    .bind(now)
    .bind(team_proposal_deadline(&proposal))
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    for mut ticket in selected {
        let previous_version = ticket.version;
        ticket.status = MatchmakingTicketStatus::Matched;
        ticket.matched_proposal_id = Some(proposal.proposal_id);
        ticket.version += 1;
        ticket.updated_at = now;
        ticket.queue_hint = None;
        let updated = sqlx::query(
            "update hepta_matchmaking_tickets
             set status='matched',matched_proposal_id=$1,version=$2,
                 record_json=$3::jsonb,updated_at=$4
             where ticket_id=$5 and status='queued' and version=$6",
        )
        .bind(proposal.proposal_id)
        .bind(
            i64::try_from(ticket.version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .bind(serde_json::to_value(&ticket).map_err(|error| {
            ApiError::internal(format!("encode automatically rematched ticket: {error}"))
        })?)
        .bind(now)
        .bind(ticket.ticket_id)
        .bind(
            i64::try_from(previous_version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .execute(&mut **tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "matchmaking_queue_changed",
                "matchmaking queue changed during automatic rematch",
            ));
        }
    }
    insert_postgres_event(
        tx,
        "auto_match_team_proposal_v1",
        &proposal.proposal_id.to_string(),
        "hepta.paper_raid.team_proposal.created.v1",
        proposal.proposal_id,
        proposal.version,
        json!({
            "proposal_id": proposal.proposal_id,
            "challenge_id": proposal.challenge_id,
            "member_player_ids": proposal.member_player_ids,
            "automatic_rematch": true,
        }),
    )
    .await?;
    Ok(Some(proposal))
}

async fn auto_match_all_queued_tickets_postgres(
    tx: &mut Transaction<'_, Postgres>,
    challenge_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<TeamProposal>, ApiError> {
    let mut proposals = Vec::new();
    while let Some(proposal) = auto_match_queued_tickets_postgres(tx, challenge_id, now).await? {
        proposals.push(proposal);
    }
    Ok(proposals)
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
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        if !eligible_player_ids.contains(&assertion.player_id) {
            return Err(ApiError::conflict(
                "matchmaking_player_not_ready",
                "matchmaking requires an active player with exactly one active Agent binding",
            ));
        }
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            request.challenge_id,
            now,
        )?;
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        expire_due_matchmaking_tickets_memory(
            &mut memory.collaboration,
            &eligible_player_ids,
            request.challenge_id,
            now,
        );
        if memory.collaboration.tickets.values().any(|ticket| {
            ticket.player_id == assertion.player_id
                && ticket.challenge_id == request.challenge_id
                && matches!(
                    ticket.status,
                    MatchmakingTicketStatus::Queued | MatchmakingTicketStatus::Matched
                )
        }) {
            return Err(ApiError::conflict(
                "live_matchmaking_ticket_exists",
                "player already has a queued or matched ticket for this challenge",
            ));
        }
        let ticket = MatchmakingTicket {
            ticket_id: request.ticket_id,
            player_id: assertion.player_id,
            challenge_id: request.challenge_id,
            requested_team_size: request.requested_team_size,
            roles: request.roles.clone(),
            availability_hash: request.availability_hash.clone(),
            party_code_hash: request.party_code_hash.clone(),
            status: MatchmakingTicketStatus::Queued,
            matched_proposal_id: None,
            expires_at: Some(now + chrono::Duration::seconds(MATCHMAKING_TICKET_TTL_SECONDS)),
            queue_hint: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        validate_premade_party_admission(
            &memory
                .collaboration
                .tickets
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            &ticket,
        )?;
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
        let selected = select_alpha_match_at(
            memory
                .collaboration
                .tickets
                .values()
                .filter(|candidate| {
                    candidate.challenge_id == request.challenge_id
                        && eligible_player_ids.contains(&candidate.player_id)
                })
                .cloned(),
            now,
        );
        let proposal = if selected.len() == 3 {
            let mut proposal = build_team_proposal(&selected)?;
            proposal.created_at = now;
            proposal.updated_at = now;
            proposal.expires_at =
                Some(now + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS));
            if reserved_team_ids.contains(&proposal.proposal_id) {
                return Err(ApiError::conflict(
                    "matchmaking_team_id_conflict",
                    "deterministic proposal team_id is already occupied by a non-matchmaking team",
                ));
            }
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
        let queue_snapshot = memory
            .collaboration
            .tickets
            .values()
            .filter(|candidate| {
                candidate.challenge_id == request.challenge_id
                    && eligible_player_ids.contains(&candidate.player_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let response_ticket = project_matchmaking_ticket(response_ticket, &queue_snapshot, now);
        let response = MatchmakingTicketResponse {
            ticket: response_ticket.clone().into(),
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
            matchmaking_ticket_created_event_payload(&response_ticket),
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
    let player_row = sqlx::query(
        "select player.record_json,
                (select count(*) from hepta_agent_bindings binding
                 where binding.player_id=player.player_id and binding.status='active')
                    as active_binding_count
         from hepta_human_players player where player.player_id = $1 for share of player",
    )
    .bind(assertion.player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden("human_player_not_registered", "human player does not exist")
    })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    if player.status != HumanPlayerStatus::Active
        || player_row.get::<i64, _>("active_binding_count") != 1
    {
        return Err(ApiError::conflict(
            "matchmaking_player_not_ready",
            "matchmaking requires an active player with exactly one active Agent binding",
        ));
    }
    let now = postgres_transaction_now(&mut tx).await?;
    expire_due_team_proposals_postgres(&mut tx, request.challenge_id, now).await?;
    expire_due_matchmaking_tickets_postgres(&mut tx, request.challenge_id, now).await?;
    if sqlx::query(
        "select 1 from hepta_matchmaking_tickets
         where ticket_id=$1
            or (player_id=$2 and challenge_id=$3 and status in ('queued','matched'))
         limit 1",
    )
    .bind(request.ticket_id)
    .bind(assertion.player_id)
    .bind(request.challenge_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .is_some()
    {
        return Err(ApiError::conflict(
            "live_matchmaking_ticket_exists",
            "player already has a queued or matched ticket for this challenge, or ticket_id exists",
        ));
    }
    let mut ticket = MatchmakingTicket {
        ticket_id: request.ticket_id,
        player_id: assertion.player_id,
        challenge_id: request.challenge_id,
        requested_team_size: request.requested_team_size,
        roles: request.roles.clone(),
        availability_hash: request.availability_hash.clone(),
        party_code_hash: request.party_code_hash.clone(),
        status: MatchmakingTicketStatus::Queued,
        matched_proposal_id: None,
        expires_at: Some(now + chrono::Duration::seconds(MATCHMAKING_TICKET_TTL_SECONDS)),
        queue_hint: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    if let Some(party_code_hash) = request.party_code_hash.as_deref() {
        sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
            .bind(postgres_party_admission_lock_key(
                request.challenge_id,
                party_code_hash,
            ))
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let live_party_tickets = sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where challenge_id=$1 and status in ('queued','matched')
               and party_code_hash=$2
             order by created_at,ticket_id",
        )
        .bind(request.challenge_id)
        .bind(party_code_hash)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
        .collect::<Result<Vec<_>, _>>()?;
        validate_premade_party_admission(&live_party_tickets, &ticket)?;
    }
    sqlx::query(
        "insert into hepta_matchmaking_tickets (
            ticket_id, player_id, challenge_id, requested_team_size, roles,
            availability_hash, party_code_hash, status, matched_proposal_id, version,
            record_json, created_at, updated_at, expires_at
         ) values ($1,$2,$3,$4,$5,$6,$7,'queued',null,1,$8::jsonb,$9,$9,$10)",
    )
    .bind(ticket.ticket_id)
    .bind(ticket.player_id)
    .bind(ticket.challenge_id)
    .bind(i32::try_from(ticket.requested_team_size).map_err(|_| {
        ApiError::bad_request("invalid_team_size", "team size exceeds PostgreSQL integer")
    })?)
    .bind(&ticket.roles)
    .bind(&ticket.availability_hash)
    .bind(ticket.party_code_hash.as_deref())
    .bind(
        serde_json::to_value(&ticket)
            .map_err(|error| ApiError::internal(format!("encode matchmaking ticket: {error}")))?,
    )
    .bind(now)
    .bind(matchmaking_ticket_deadline(&ticket))
    .execute(&mut *tx)
    .await
    .map_err(|error| match &error {
        sqlx::Error::Database(database) if database.is_unique_violation() => ApiError::conflict(
            "live_matchmaking_ticket_exists",
            "player already has a queued or matched ticket, or ticket_id exists",
        ),
        _ => ApiError::database(error),
    })?;
    let rows = sqlx::query(
        "select ticket.version,ticket.record_json
         from hepta_matchmaking_tickets ticket
         join hepta_human_players player on player.player_id=ticket.player_id
         where ticket.challenge_id = $1 and ticket.requested_team_size = 3
           and ticket.status = 'queued' and player.status = 'active'
           and (select count(*) from hepta_agent_bindings binding
                where binding.player_id=ticket.player_id and binding.status='active') = 1
         order by ticket.created_at, ticket.ticket_id
         limit 2048
         for update of ticket for key share of player",
    )
    .bind(request.challenge_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let queue_snapshot = rows
        .into_iter()
        .map(|row| {
            let version = u64::try_from(row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
            let ticket: MatchmakingTicket =
                decode_record(row.get("record_json"), "matchmaking ticket")?;
            if ticket.version != version {
                return Err(ApiError::internal(
                    "matchmaking ticket record/version projection diverged",
                ));
            }
            Ok(ticket)
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let selected = select_alpha_match_at(queue_snapshot.clone().into_iter(), now);
    let proposal = if selected.len() == 3 {
        let mut proposal = build_team_proposal(&selected)?;
        proposal.created_at = now;
        proposal.updated_at = now;
        proposal.expires_at =
            Some(now + chrono::Duration::seconds(TEAM_PROPOSAL_RESPONSE_TTL_SECONDS));
        sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("hepta-paper-raid-team-id:{}", proposal.proposal_id))
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        if sqlx::query("select 1 from hepta_research_teams where team_id=$1 limit 1")
            .bind(proposal.proposal_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .is_some()
        {
            return Err(ApiError::conflict(
                "matchmaking_team_id_conflict",
                "deterministic proposal team_id is already occupied by a non-matchmaking team",
            ));
        }
        sqlx::query(
            "insert into hepta_team_proposals (
                proposal_id, challenge_id, requested_team_size, deterministic_match_key,
                status, member_player_ids, source_ticket_ids, solver_version,
                source_preferences, role_assignments, version, record_json,
                created_at, updated_at, expires_at
             ) values ($1,$2,3,$3,'proposed',$4,$5,$6,$7::jsonb,$8::jsonb,1,$9::jsonb,$10,$10,$11)",
        )
        .bind(proposal.proposal_id)
        .bind(proposal.challenge_id)
        .bind(&proposal.deterministic_match_key)
        .bind(&proposal.member_player_ids)
        .bind(&proposal.source_ticket_ids)
        .bind(proposal.solver_version.as_deref())
        .bind(
            serde_json::to_value(&proposal.source_preferences).map_err(|error| {
                ApiError::internal(format!("encode team proposal source preferences: {error}"))
            })?,
        )
        .bind(
            serde_json::to_value(&proposal.role_assignments).map_err(|error| {
                ApiError::internal(format!("encode team proposal role assignments: {error}"))
            })?,
        )
        .bind(
            serde_json::to_value(&proposal)
                .map_err(|error| ApiError::internal(format!("encode team proposal: {error}")))?,
        )
        .bind(now)
        .bind(team_proposal_deadline(&proposal))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        for mut selected_ticket in selected {
            let expected_version = selected_ticket.version;
            selected_ticket.status = MatchmakingTicketStatus::Matched;
            selected_ticket.matched_proposal_id = Some(proposal.proposal_id);
            selected_ticket.version += 1;
            selected_ticket.updated_at = now;
            let updated =
                sqlx::query(
                    "update hepta_matchmaking_tickets
                 set status = 'matched', matched_proposal_id = $1,
                     version = version + 1, record_json = $2::jsonb, updated_at = $3
                 where ticket_id = $4 and status = 'queued' and version = $5",
                )
                .bind(proposal.proposal_id)
                .bind(serde_json::to_value(&selected_ticket).map_err(|error| {
                    ApiError::internal(format!("encode matched ticket: {error}"))
                })?)
                .bind(now)
                .bind(selected_ticket.ticket_id)
                .bind(
                    i64::try_from(expected_version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
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
    let response_queue_snapshot = sqlx::query(
        "select ticket.record_json from hepta_matchmaking_tickets ticket
         join hepta_human_players player on player.player_id=ticket.player_id
         where ticket.challenge_id=$1 and ticket.status='queued'
           and player.status='active'
           and (select count(*) from hepta_agent_bindings binding
                where binding.player_id=ticket.player_id and binding.status='active') = 1
         order by ticket.created_at,ticket.ticket_id",
    )
    .bind(request.challenge_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
    .collect::<Result<Vec<_>, _>>()?;
    let response = MatchmakingTicketResponse {
        ticket: project_matchmaking_ticket(ticket.clone(), &response_queue_snapshot, now).into(),
        team_proposal: proposal,
    };
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.matchmaking_ticket.created.v1",
        ticket.ticket_id,
        ticket.version,
        matchmaking_ticket_created_event_payload(&ticket),
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

async fn cancel_matchmaking_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_id): Path<Uuid>,
    Json(request): Json<CancelMatchmakingTicketRequest>,
) -> Result<(StatusCode, Json<MatchmakingTicketView>), ApiError> {
    const OPERATION: &str = "cancel_matchmaking_ticket_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/matchmaking/tickets/{ticket_id}/cancel");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden("human_player_not_registered", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        let snapshot = memory
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
            })?;
        if snapshot.version != request.expected_version {
            return Err(version_conflict(
                "matchmaking ticket",
                request.expected_version,
                snapshot.version,
            ));
        }
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        )?;
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        let expired_tickets = expire_due_matchmaking_tickets_memory(
            &mut memory.collaboration,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        );
        if !expiry_sweep.expired.is_empty()
            || !expiry_sweep.replacements.is_empty()
            || !expired_tickets.is_empty()
        {
            *memory_guard = memory.clone();
        }
        let current = memory
            .collaboration
            .tickets
            .get(&ticket_id)
            .cloned()
            .expect("visible ticket remains stored");
        if current.version != request.expected_version {
            *memory_guard = memory;
            return Err(version_conflict(
                "matchmaking ticket",
                request.expected_version,
                current.version,
            ));
        }
        let mut withdrawn_proposal = None;
        match current.status {
            MatchmakingTicketStatus::Queued => {
                let ticket = memory
                    .collaboration
                    .tickets
                    .get_mut(&ticket_id)
                    .expect("visible queued ticket remains stored");
                ticket.status = MatchmakingTicketStatus::Cancelled;
                ticket.matched_proposal_id = None;
                ticket.version += 1;
                ticket.updated_at = now;
                ticket.queue_hint = None;
            }
            MatchmakingTicketStatus::Matched => {
                let proposal_id = current.matched_proposal_id.ok_or_else(|| {
                    ApiError::internal("matched ticket is missing its team proposal")
                })?;
                if memory.teams.contains_key(&proposal_id) {
                    return Err(ApiError::internal(
                        "matched ticket has a same-ID team without consumed materialization authority",
                    ));
                }
                let mut proposal = memory
                    .collaboration
                    .team_proposals
                    .get(&proposal_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("matched team proposal is missing"))?;
                if !matches!(
                    proposal.status,
                    TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
                ) {
                    return Err(ApiError::conflict(
                        "matchmaking_ticket_not_cancellable",
                        "matched proposal is already closed",
                    ));
                }
                for source_ticket_id in &proposal.source_ticket_ids {
                    let ticket = memory
                        .collaboration
                        .tickets
                        .get_mut(source_ticket_id)
                        .ok_or_else(|| ApiError::internal("team proposal source ticket missing"))?;
                    ticket.status = if ticket.player_id == assertion.player_id {
                        MatchmakingTicketStatus::Cancelled
                    } else if matchmaking_ticket_deadline(ticket) <= now
                        || !eligible_player_ids.contains(&ticket.player_id)
                    {
                        MatchmakingTicketStatus::Expired
                    } else {
                        MatchmakingTicketStatus::Queued
                    };
                    ticket.matched_proposal_id = None;
                    ticket.version += 1;
                    ticket.updated_at = now;
                    ticket.queue_hint = None;
                }
                proposal.status = TeamProposalStatus::Declined;
                proposal.version += 1;
                proposal.updated_at = now;
                memory
                    .collaboration
                    .team_proposals
                    .insert(proposal_id, proposal.clone());
                withdrawn_proposal = Some(proposal);
            }
            MatchmakingTicketStatus::Consumed => {
                return Err(ApiError::conflict(
                    "materialized_team_cannot_withdraw",
                    "matchmaking ticket was consumed by a materialized team",
                ));
            }
            MatchmakingTicketStatus::Cancelled | MatchmakingTicketStatus::Expired => {
                return Err(ApiError::conflict(
                    "matchmaking_ticket_not_cancellable",
                    "cancelled or expired matchmaking tickets are immutable",
                ));
            }
        }
        let response = memory
            .collaboration
            .tickets
            .get(&ticket_id)
            .cloned()
            .expect("cancelled ticket remains stored");
        if let Some(proposal) = withdrawn_proposal {
            let replacements = auto_match_all_queued_tickets_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                proposal.challenge_id,
                now,
            )?;
            push_memory_event(
                &mut memory,
                OPERATION,
                &format!("{}:proposal-withdrawn", request.idempotency_key),
                "hepta.paper_raid.team_proposal.withdrawn.v1",
                proposal.proposal_id,
                proposal.version,
                json!({
                    "proposal_id": proposal.proposal_id,
                    "challenge_id": proposal.challenge_id,
                    "withdrawing_player_id": assertion.player_id,
                    "status": proposal.status,
                }),
            )?;
            push_automatic_team_proposal_events_memory(&mut memory, &replacements)?;
        }
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.matchmaking_ticket.cancelled.v1",
            ticket_id,
            response.version,
            matchmaking_ticket_cancelled_event_payload(&response),
        )?;
        let response_view = MatchmakingTicketView::from(response);
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response_view,
        )?;
        *memory_guard = memory;
        return Ok((StatusCode::OK, Json(response_view)));
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
    let snapshot_row = sqlx::query(
        "select version,record_json from hepta_matchmaking_tickets
         where ticket_id=$1 and player_id=$2",
    )
    .bind(ticket_id)
    .bind(assertion.player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "matchmaking_ticket_not_found",
            "visible matchmaking ticket does not exist",
        )
    })?;
    let snapshot: MatchmakingTicket =
        decode_record(snapshot_row.get("record_json"), "matchmaking ticket")?;
    let snapshot_version = u64::try_from(snapshot_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
    if snapshot_version != request.expected_version || snapshot.version != snapshot_version {
        return Err(version_conflict(
            "matchmaking ticket",
            request.expected_version,
            snapshot_version,
        ));
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-paper-raid-matchmaking:{}",
            snapshot.challenge_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let now = postgres_transaction_now(&mut tx).await?;
    expire_due_team_proposals_postgres(&mut tx, snapshot.challenge_id, now).await?;
    expire_due_matchmaking_tickets_postgres(&mut tx, snapshot.challenge_id, now).await?;
    let row = sqlx::query(
        "select version,record_json from hepta_matchmaking_tickets
         where ticket_id=$1 and player_id=$2 for update",
    )
    .bind(ticket_id)
    .bind(assertion.player_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let mut ticket: MatchmakingTicket =
        decode_record(row.get("record_json"), "matchmaking ticket")?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
    if ticket.version != actual_version {
        return Err(ApiError::internal(
            "matchmaking ticket record/version projection diverged",
        ));
    }
    if actual_version != request.expected_version {
        tx.commit().await.map_err(ApiError::database)?;
        return Err(version_conflict(
            "matchmaking ticket",
            request.expected_version,
            actual_version,
        ));
    }
    let mut withdrawn_challenge_id = None;
    match ticket.status {
        MatchmakingTicketStatus::Queued => {
            ticket.status = MatchmakingTicketStatus::Cancelled;
            ticket.matched_proposal_id = None;
            ticket.version += 1;
            ticket.updated_at = now;
            ticket.queue_hint = None;
            let updated =
                sqlx::query(
                    "update hepta_matchmaking_tickets
                 set status='cancelled',matched_proposal_id=null,version=$1,
                     record_json=$2::jsonb,updated_at=$3
                 where ticket_id=$4 and player_id=$5 and status='queued' and version=$6",
                )
                .bind(
                    i64::try_from(ticket.version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .bind(serde_json::to_value(&ticket).map_err(|error| {
                    ApiError::internal(format!("encode cancelled ticket: {error}"))
                })?)
                .bind(now)
                .bind(ticket.ticket_id)
                .bind(ticket.player_id)
                .bind(
                    i64::try_from(actual_version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "matchmaking_queue_changed",
                    "matchmaking ticket changed while it was being cancelled",
                ));
            }
        }
        MatchmakingTicketStatus::Matched => {
            let proposal_id = ticket
                .matched_proposal_id
                .ok_or_else(|| ApiError::internal("matched ticket is missing its team proposal"))?;
            let proposal_row = sqlx::query(
                "select version,record_json from hepta_team_proposals
                 where proposal_id=$1 for update",
            )
            .bind(proposal_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
            let materialized =
                sqlx::query("select 1 from hepta_research_teams where team_id=$1 limit 1")
                    .bind(proposal_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(ApiError::database)?
                    .is_some();
            if materialized {
                return Err(ApiError::internal(
                    "matched ticket has a same-ID team without consumed materialization authority",
                ));
            }
            let proposal_version = u64::try_from(proposal_row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("team proposal version is invalid"))?;
            let mut proposal: TeamProposal =
                decode_record(proposal_row.get("record_json"), "team proposal")?;
            if proposal.version != proposal_version
                || !matches!(
                    proposal.status,
                    TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
                )
            {
                return Err(ApiError::conflict(
                    "matchmaking_ticket_not_cancellable",
                    "matched proposal is already closed",
                ));
            }
            let rows = sqlx::query(
                "select ticket.version,ticket.record_json,player.status as player_status,
                        (select count(*) from hepta_agent_bindings binding
                         where binding.player_id=ticket.player_id and binding.status='active')
                            as active_binding_count
                 from hepta_matchmaking_tickets ticket
                 join hepta_human_players player on player.player_id=ticket.player_id
                 where ticket.ticket_id=any($1) order by ticket.ticket_id
                 for update of ticket for key share of player",
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
            for source_row in rows {
                let source_version = u64::try_from(source_row.get::<i64, _>("version"))
                    .map_err(|_| ApiError::internal("ticket version is invalid"))?;
                let mut source: MatchmakingTicket =
                    decode_record(source_row.get("record_json"), "matchmaking ticket")?;
                if source.version != source_version
                    || source.status != MatchmakingTicketStatus::Matched
                    || source.matched_proposal_id != Some(proposal_id)
                {
                    return Err(ApiError::internal(
                        "team proposal source ticket projection diverged",
                    ));
                }
                source.status = if source.player_id == assertion.player_id {
                    MatchmakingTicketStatus::Cancelled
                } else if matchmaking_ticket_deadline(&source) <= now
                    || source_row.get::<String, _>("player_status") != "active"
                    || source_row.get::<i64, _>("active_binding_count") != 1
                {
                    MatchmakingTicketStatus::Expired
                } else {
                    MatchmakingTicketStatus::Queued
                };
                source.matched_proposal_id = None;
                source.version += 1;
                source.updated_at = now;
                source.queue_hint = None;
                let updated = sqlx::query(
                    "update hepta_matchmaking_tickets
                     set status=$1,matched_proposal_id=null,version=$2,
                         record_json=$3::jsonb,updated_at=$4
                     where ticket_id=$5 and status='matched' and matched_proposal_id=$6
                       and version=$7",
                )
                .bind(source.status.as_str())
                .bind(
                    i64::try_from(source.version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .bind(serde_json::to_value(&source).map_err(|error| {
                    ApiError::internal(format!("encode withdrawn proposal ticket: {error}"))
                })?)
                .bind(now)
                .bind(source.ticket_id)
                .bind(proposal_id)
                .bind(
                    i64::try_from(source_version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
                if updated.rows_affected() != 1 {
                    return Err(ApiError::conflict(
                        "matchmaking_queue_changed",
                        "team proposal ticket changed while a member withdrew",
                    ));
                }
                if source.ticket_id == ticket_id {
                    ticket = source;
                }
            }
            proposal.status = TeamProposalStatus::Declined;
            proposal.version += 1;
            proposal.updated_at = now;
            let updated = sqlx::query(
                "update hepta_team_proposals
                 set status='declined',version=$1,record_json=$2::jsonb,updated_at=$3
                 where proposal_id=$4 and version=$5 and status in ('proposed','accepted')",
            )
            .bind(
                i64::try_from(proposal.version)
                    .map_err(|_| ApiError::internal("proposal version overflow"))?,
            )
            .bind(serde_json::to_value(&proposal).map_err(|error| {
                ApiError::internal(format!("encode withdrawn team proposal: {error}"))
            })?)
            .bind(now)
            .bind(proposal_id)
            .bind(
                i64::try_from(proposal_version)
                    .map_err(|_| ApiError::internal("proposal version overflow"))?,
            )
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "team_proposal_withdrawal_race",
                    "team proposal changed while a member withdrew",
                ));
            }
            insert_postgres_event(
                &mut tx,
                OPERATION,
                &format!("{}:proposal-withdrawn", request.idempotency_key),
                "hepta.paper_raid.team_proposal.withdrawn.v1",
                proposal_id,
                proposal.version,
                json!({
                    "proposal_id": proposal_id,
                    "challenge_id": proposal.challenge_id,
                    "withdrawing_player_id": assertion.player_id,
                    "status": proposal.status,
                }),
            )
            .await?;
            withdrawn_challenge_id = Some(proposal.challenge_id);
        }
        MatchmakingTicketStatus::Consumed => {
            return Err(ApiError::conflict(
                "materialized_team_cannot_withdraw",
                "matchmaking ticket was consumed by a materialized team",
            ));
        }
        MatchmakingTicketStatus::Cancelled | MatchmakingTicketStatus::Expired => {
            return Err(ApiError::conflict(
                "matchmaking_ticket_not_cancellable",
                "cancelled or expired matchmaking tickets are immutable",
            ));
        }
    }
    if let Some(challenge_id) = withdrawn_challenge_id {
        auto_match_all_queued_tickets_postgres(&mut tx, challenge_id, now).await?;
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.matchmaking_ticket.cancelled.v1",
        ticket.ticket_id,
        ticket.version,
        matchmaking_ticket_cancelled_event_payload(&ticket),
    )
    .await?;
    let response = MatchmakingTicketView::from(ticket.clone());
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(ticket.ticket_id),
        StatusCode::OK,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(response)))
}

async fn list_matchmaking_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MatchmakingTicketView>>, ApiError> {
    let assertion = require_registered_player_read(
        &state,
        &headers,
        "list_matchmaking_tickets_v3",
        "/v2/hepta/matchmaking/tickets",
    )
    .await?;
    let (mut tickets, queue_snapshot, projection_now) = if let Some(pool) = &state.pool {
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        let mut challenge_ids = sqlx::query(
            "select distinct challenge_id from hepta_matchmaking_tickets where player_id=$1",
        )
        .bind(assertion.player_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| row.get::<Uuid, _>("challenge_id"))
        .collect::<Vec<_>>();
        challenge_ids.sort_unstable();
        let storage_now = postgres_transaction_now(&mut tx).await?;
        for challenge_id in &challenge_ids {
            sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
                .bind(format!("hepta-paper-raid-matchmaking:{challenge_id}"))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            expire_due_team_proposals_postgres(&mut tx, *challenge_id, storage_now).await?;
            expire_due_matchmaking_tickets_postgres(&mut tx, *challenge_id, storage_now).await?;
            // Migration 0046 may have released legacy proposal tickets before
            // this process started.  Every player-scoped read drains all
            // currently matchable queued triplets even when no fresh expiry
            // or mutation occurred in this request.
            auto_match_all_queued_tickets_postgres(&mut tx, *challenge_id, storage_now).await?;
        }
        let tickets = sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where player_id=$1 order by created_at,ticket_id",
        )
        .bind(assertion.player_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
        .collect::<Result<Vec<_>, _>>()?;
        let queue = if challenge_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query(
                "select ticket.record_json
                 from hepta_matchmaking_tickets ticket
                 join hepta_human_players player on player.player_id=ticket.player_id
                 where ticket.challenge_id=any($1) and ticket.status='queued'
                   and player.status='active'
                   and (select count(*) from hepta_agent_bindings binding
                        where binding.player_id=ticket.player_id and binding.status='active') = 1
                 order by ticket.created_at,ticket.ticket_id",
            )
            .bind(&challenge_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .into_iter()
            .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
            .collect::<Result<Vec<_>, _>>()?
        };
        let projection_now = storage_now;
        tx.commit().await.map_err(ApiError::database)?;
        (tickets, queue, projection_now)
    } else {
        let mut memory = state.paper_raid.write().await;
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        let challenge_ids = memory
            .collaboration
            .tickets
            .values()
            .filter(|ticket| ticket.player_id == assertion.player_id)
            .map(|ticket| ticket.challenge_id)
            .collect::<HashSet<_>>();
        for challenge_id in &challenge_ids {
            let expiry_sweep = expire_due_team_proposals_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                *challenge_id,
                now,
            )?;
            push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
            expire_due_matchmaking_tickets_memory(
                &mut memory.collaboration,
                &eligible_player_ids,
                *challenge_id,
                now,
            );
            let replacements = auto_match_all_queued_tickets_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                *challenge_id,
                now,
            )?;
            push_automatic_team_proposal_events_memory(&mut memory, &replacements)?;
        }
        let tickets = memory
            .collaboration
            .tickets
            .values()
            .filter(|ticket| ticket.player_id == assertion.player_id)
            .cloned()
            .collect::<Vec<_>>();
        let queue = memory
            .collaboration
            .tickets
            .values()
            .filter(|ticket| {
                challenge_ids.contains(&ticket.challenge_id)
                    && eligible_player_ids.contains(&ticket.player_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        (tickets, queue, now)
    };
    tickets.sort_by_key(|ticket| (ticket.created_at, ticket.ticket_id));
    tickets = tickets
        .into_iter()
        .map(|ticket| project_matchmaking_ticket(ticket, &queue_snapshot, projection_now))
        .collect();
    Ok(Json(tickets.into_iter().map(Into::into).collect()))
}

async fn get_matchmaking_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<MatchmakingTicketView>, ApiError> {
    let path = format!("/v2/hepta/matchmaking/tickets/{ticket_id}");
    let assertion =
        require_registered_player_read(&state, &headers, "get_matchmaking_ticket_v3", &path)
            .await?;
    let (ticket, queue_snapshot, projection_now) = if let Some(pool) = &state.pool {
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        let row = sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where ticket_id = $1 and player_id = $2",
        )
        .bind(ticket_id)
        .bind(assertion.player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "matchmaking_ticket_not_found",
                "visible matchmaking ticket does not exist",
            )
        })?;
        let snapshot: MatchmakingTicket =
            decode_record(row.get("record_json"), "matchmaking ticket")?;
        sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
            .bind(format!(
                "hepta-paper-raid-matchmaking:{}",
                snapshot.challenge_id
            ))
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let now = postgres_transaction_now(&mut tx).await?;
        expire_due_team_proposals_postgres(&mut tx, snapshot.challenge_id, now).await?;
        expire_due_matchmaking_tickets_postgres(&mut tx, snapshot.challenge_id, now).await?;
        auto_match_all_queued_tickets_postgres(&mut tx, snapshot.challenge_id, now).await?;
        let row = sqlx::query(
            "select record_json from hepta_matchmaking_tickets
             where ticket_id=$1 and player_id=$2",
        )
        .bind(ticket_id)
        .bind(assertion.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        let ticket = decode_record(row.get("record_json"), "matchmaking ticket")?;
        let queue = sqlx::query(
            "select ticket.record_json
             from hepta_matchmaking_tickets ticket
             join hepta_human_players player on player.player_id=ticket.player_id
             where ticket.challenge_id=$1 and ticket.status='queued'
               and player.status='active'
               and (select count(*) from hepta_agent_bindings binding
                    where binding.player_id=ticket.player_id and binding.status='active') = 1
             order by ticket.created_at,ticket.ticket_id",
        )
        .bind(snapshot.challenge_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "matchmaking ticket"))
        .collect::<Result<Vec<_>, _>>()?;
        let projection_now = now;
        tx.commit().await.map_err(ApiError::database)?;
        (ticket, queue, projection_now)
    } else {
        let mut memory = state.paper_raid.write().await;
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        let snapshot = memory
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
            })?;
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        )?;
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        expire_due_matchmaking_tickets_memory(
            &mut memory.collaboration,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        );
        let replacements = auto_match_all_queued_tickets_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        )?;
        push_automatic_team_proposal_events_memory(&mut memory, &replacements)?;
        let ticket = memory
            .collaboration
            .tickets
            .get(&ticket_id)
            .cloned()
            .expect("visible ticket remains stored");
        let queue = memory
            .collaboration
            .tickets
            .values()
            .filter(|candidate| {
                candidate.challenge_id == snapshot.challenge_id
                    && eligible_player_ids.contains(&candidate.player_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        (ticket, queue, now)
    };
    Ok(Json(
        project_matchmaking_ticket(ticket, &queue_snapshot, projection_now).into(),
    ))
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
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        let mut challenge_ids = sqlx::query(
            "select distinct challenge_id from hepta_team_proposals
             where $1 = any(member_player_ids) and status in ('proposed','accepted')",
        )
        .bind(assertion.player_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| row.get::<Uuid, _>("challenge_id"))
        .collect::<Vec<_>>();
        challenge_ids.sort_unstable();
        let storage_now = postgres_transaction_now(&mut tx).await?;
        for challenge_id in challenge_ids {
            sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
                .bind(format!("hepta-paper-raid-matchmaking:{challenge_id}"))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
            expire_due_team_proposals_postgres(&mut tx, challenge_id, storage_now).await?;
        }
        let proposals = sqlx::query(
            "select record_json from hepta_team_proposals
             where $1 = any(member_player_ids) and status in ('proposed','accepted')
             order by created_at, proposal_id",
        )
        .bind(assertion.player_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "team proposal"))
        .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await.map_err(ApiError::database)?;
        proposals
    } else {
        let mut memory = state.paper_raid.write().await;
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        let challenge_ids = memory
            .collaboration
            .team_proposals
            .values()
            .filter(|proposal| {
                proposal.member_player_ids.contains(&assertion.player_id)
                    && matches!(
                        proposal.status,
                        TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
                    )
            })
            .map(|proposal| proposal.challenge_id)
            .collect::<HashSet<_>>();
        for challenge_id in challenge_ids {
            let expiry_sweep = expire_due_team_proposals_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                challenge_id,
                now,
            )?;
            push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        }
        memory
            .collaboration
            .team_proposals
            .values()
            .filter(|proposal| {
                proposal.member_player_ids.contains(&assertion.player_id)
                    && matches!(
                        proposal.status,
                        TeamProposalStatus::Proposed | TeamProposalStatus::Accepted
                    )
            })
            .cloned()
            .collect()
    };
    proposals = proposals
        .into_iter()
        .map(project_team_proposal_deadline)
        .collect();
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
        let mut tx = pool.begin().await.map_err(ApiError::database)?;
        let row = sqlx::query(
            "select record_json from hepta_team_proposals
             where proposal_id = $1 and $2 = any(member_player_ids)",
        )
        .bind(proposal_id)
        .bind(assertion.player_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "team_proposal_not_found",
                "visible team proposal does not exist",
            )
        })?;
        let snapshot: TeamProposal = decode_record(row.get("record_json"), "team proposal")?;
        sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
            .bind(format!(
                "hepta-paper-raid-matchmaking:{}",
                snapshot.challenge_id
            ))
            .execute(&mut *tx)
            .await
            .map_err(ApiError::database)?;
        let storage_now = postgres_transaction_now(&mut tx).await?;
        expire_due_team_proposals_postgres(&mut tx, snapshot.challenge_id, storage_now).await?;
        let row = sqlx::query(
            "select record_json from hepta_team_proposals
             where proposal_id = $1 and $2 = any(member_player_ids)",
        )
        .bind(proposal_id)
        .bind(assertion.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        let proposal = decode_record(row.get("record_json"), "team proposal")?;
        tx.commit().await.map_err(ApiError::database)?;
        proposal
    } else {
        let mut memory = state.paper_raid.write().await;
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        let snapshot = memory
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
            })?;
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        )?;
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        memory
            .collaboration
            .team_proposals
            .get(&snapshot.proposal_id)
            .cloned()
            .expect("visible proposal remains stored")
    };
    Ok(Json(project_team_proposal_deadline(proposal)))
}

fn first_playable_team(
    proposal: &TeamProposal,
    tickets: &[MatchmakingTicket],
    bindings: &[AgentBinding],
    now: DateTime<Utc>,
) -> Result<ResearchTeam, ApiError> {
    let all_matched = tickets
        .iter()
        .all(|ticket| ticket.status == MatchmakingTicketStatus::Matched);
    let all_consumed = tickets
        .iter()
        .all(|ticket| ticket.status == MatchmakingTicketStatus::Consumed);
    if proposal.status != TeamProposalStatus::Accepted
        || proposal.requested_team_size != 3
        || proposal.member_player_ids.len() != 3
        || proposal.source_ticket_ids.len() != 3
        || tickets.len() != 3
        || bindings.len() != 3
        || !(all_matched || all_consumed)
    {
        return Err(ApiError::conflict(
            "team_proposal_not_materializable",
            "first-playable materialization requires one accepted three-player proposal with exact tickets and bindings",
        ));
    }
    let ordered_tickets = proposal
        .source_ticket_ids
        .iter()
        .map(|ticket_id| {
            tickets
                .iter()
                .find(|ticket| ticket.ticket_id == *ticket_id)
                .cloned()
                .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let assigned_roles = validate_materialization_matchmaking_source(proposal, &ordered_tickets)?;
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
        {
            return Err(ApiError::conflict(
                "team_proposal_ticket_mismatch",
                "team proposal no longer matches its exact player tickets",
            ));
        }
        let role = assigned_roles[index].clone();
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
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
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
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            proposal.challenge_id,
            now,
        )?;
        let current_expired = expiry_sweep
            .expired
            .iter()
            .any(|expired| expired.proposal_id == proposal_id);
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        if !expiry_sweep.expired.is_empty() || !expiry_sweep.replacements.is_empty() {
            *memory_guard = memory.clone();
        }
        if current_expired {
            *memory_guard = memory;
            return Err(ApiError::conflict(
                "team_proposal_expired",
                "team proposal expired before materialization because its response window elapsed or a member lost matchmaking eligibility",
            ));
        }
        let proposal = memory
            .collaboration
            .team_proposals
            .get(&proposal_id)
            .cloned()
            .expect("visible proposal remains stored after deadline sweep");
        if proposal.version != request.expected_proposal_version {
            *memory_guard = memory;
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
        if let Err(error) = validate_materialization_matchmaking_source(&proposal, &tickets) {
            let invalidation = invalidate_team_proposal_provenance_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                proposal.proposal_id,
                now,
            )?;
            push_team_proposal_expiry_sweep_memory(&mut memory, &invalidation)?;
            *memory_guard = memory;
            return Err(error);
        }
        if proposal
            .member_player_ids
            .iter()
            .any(|player_id| !eligible_player_ids.contains(player_id))
        {
            return Err(ApiError::conflict(
                "matchmaking_player_not_ready",
                "team materialization requires every player to remain active with exactly one active Agent binding",
            ));
        }
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
        if memory.teams.contains_key(&proposal_id) {
            return Err(ApiError::conflict(
                "team_materialization_provenance_conflict",
                "proposal-derived team_id already exists without this exact idempotent materialization",
            ));
        }
        if tickets
            .iter()
            .all(|ticket| ticket.status == MatchmakingTicketStatus::Consumed)
        {
            return Err(ApiError::internal(
                "consumed matchmaking tickets have no materialized team",
            ));
        }
        let expected = first_playable_team(&proposal, &tickets, &bindings, now)?;
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
        let status = StatusCode::CREATED;
        let team = expected;
        let mut consumed = false;
        for ticket_id in &proposal.source_ticket_ids {
            let ticket = memory
                .collaboration
                .tickets
                .get_mut(ticket_id)
                .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))?;
            consumed |= consume_materialized_ticket(ticket, proposal.proposal_id, now)?;
        }
        if consumed {
            push_memory_event(
                &mut memory,
                OPERATION,
                &format!("{}:tickets-consumed", request.idempotency_key),
                "hepta.paper_raid.matchmaking_tickets.consumed.v1",
                team.team_id,
                team.version,
                json!({
                    "proposal_id": proposal.proposal_id,
                    "team_id": team.team_id,
                    "source_ticket_ids": &proposal.source_ticket_ids,
                }),
            )?;
        }
        let stored_proposal = memory
            .collaboration
            .team_proposals
            .get_mut(&proposal.proposal_id)
            .expect("materialized proposal remains stored");
        if stored_proposal.status != TeamProposalStatus::Accepted
            || stored_proposal.version != proposal.version
        {
            return Err(ApiError::conflict(
                "team_materialization_proposal_race",
                "accepted proposal changed while its team materialized",
            ));
        }
        stored_proposal.status = TeamProposalStatus::Materialized;
        stored_proposal.version += 1;
        stored_proposal.updated_at = now;
        let materialized_proposal = stored_proposal.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &format!("{}:proposal-materialized", request.idempotency_key),
            "hepta.paper_raid.team_proposal.materialized.v1",
            materialized_proposal.proposal_id,
            materialized_proposal.version,
            json!({
                "proposal_id": materialized_proposal.proposal_id,
                "team_id": team.team_id,
                "status": materialized_proposal.status,
            }),
        )?;
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
    let proposal_snapshot_row = sqlx::query(
        "select challenge_id,record_json from hepta_team_proposals where proposal_id=$1",
    )
    .bind(proposal_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
    })?;
    let proposal_snapshot: TeamProposal =
        decode_record(proposal_snapshot_row.get("record_json"), "team proposal")?;
    if !proposal_snapshot
        .member_player_ids
        .contains(&assertion.player_id)
    {
        return Err(ApiError::forbidden(
            "not_team_proposal_member",
            "only an accepted proposal member can materialize its team",
        ));
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-paper-raid-matchmaking:{}",
            proposal_snapshot.challenge_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("hepta-paper-raid-team-id:{proposal_id}"))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let now = postgres_transaction_now(&mut tx).await?;
    let expired =
        expire_due_team_proposals_postgres(&mut tx, proposal_snapshot.challenge_id, now).await?;
    if expired
        .iter()
        .any(|proposal| proposal.proposal_id == proposal_id)
    {
        tx.commit().await.map_err(ApiError::database)?;
        return Err(ApiError::conflict(
            "team_proposal_expired",
            "team proposal expired before materialization because its response window elapsed or a member lost matchmaking eligibility",
        ));
    }
    let proposal_row =
        sqlx::query("select record_json from hepta_team_proposals where proposal_id=$1 for update")
            .bind(proposal_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let mut proposal: TeamProposal =
        decode_record(proposal_row.get("record_json"), "team proposal")?;
    if !proposal.member_player_ids.contains(&assertion.player_id) {
        return Err(ApiError::forbidden(
            "not_team_proposal_member",
            "only an accepted proposal member can materialize its team",
        ));
    }
    if proposal.version != request.expected_proposal_version {
        tx.commit().await.map_err(ApiError::database)?;
        return Err(version_conflict(
            "team proposal",
            request.expected_proposal_version,
            proposal.version,
        ));
    }
    let rows = sqlx::query(
        "select version,record_json from hepta_matchmaking_tickets
         where ticket_id = any($1) order by ticket_id for update",
    )
    .bind(&proposal.source_ticket_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let decoded = rows
        .into_iter()
        .map(|row| {
            let version = u64::try_from(row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
            let ticket: MatchmakingTicket =
                decode_record(row.get("record_json"), "matchmaking ticket")?;
            if ticket.version != version {
                return Err(ApiError::internal(
                    "matchmaking ticket record/version projection diverged",
                ));
            }
            Ok(ticket)
        })
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
    if let Err(error) = validate_materialization_matchmaking_source(&proposal, &tickets) {
        invalidate_team_proposal_provenance_postgres(&mut tx, &proposal, now).await?;
        tx.commit().await.map_err(ApiError::database)?;
        return Err(error);
    }
    let mut bindings = Vec::with_capacity(3);
    for player_id in &proposal.member_player_ids {
        let rows = sqlx::query(
            "select b.record_json from hepta_agent_bindings b
             join hepta_human_players p on p.player_id=b.player_id
             where b.player_id=$1 and b.status='active' and p.status='active'
             order by b.updated_at desc, b.binding_id
             for share of p,b",
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
    let existing = sqlx::query("select 1 from hepta_research_teams where team_id=$1 for update")
        .bind(proposal.proposal_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    if existing.is_some() {
        return Err(ApiError::conflict(
            "team_materialization_provenance_conflict",
            "proposal-derived team_id already exists without this exact idempotent materialization",
        ));
    }
    if tickets
        .iter()
        .all(|ticket| ticket.status == MatchmakingTicketStatus::Consumed)
    {
        return Err(ApiError::internal(
            "consumed matchmaking tickets have no materialized team",
        ));
    }
    let expected = first_playable_team(&proposal, &tickets, &bindings, now)?;
    let (status, team) =
        {
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
    let mut consumed = false;
    for mut ticket in tickets {
        let previous_version = ticket.version;
        if !consume_materialized_ticket(&mut ticket, proposal.proposal_id, now)? {
            continue;
        }
        consumed = true;
        let updated = sqlx::query(
            "update hepta_matchmaking_tickets
             set status='consumed',version=$1,record_json=$2::jsonb,updated_at=$3
             where ticket_id=$4 and status='matched' and matched_proposal_id=$5 and version=$6",
        )
        .bind(
            i64::try_from(ticket.version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .bind(serde_json::to_value(&ticket).map_err(|error| {
            ApiError::internal(format!("encode consumed matchmaking ticket: {error}"))
        })?)
        .bind(now)
        .bind(ticket.ticket_id)
        .bind(proposal.proposal_id)
        .bind(
            i64::try_from(previous_version)
                .map_err(|_| ApiError::internal("ticket version overflow"))?,
        )
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "team_materialization_ticket_race",
                "source ticket changed while the accepted team materialized",
            ));
        }
    }
    if consumed {
        insert_postgres_event(
            &mut tx,
            OPERATION,
            &format!("{}:tickets-consumed", request.idempotency_key),
            "hepta.paper_raid.matchmaking_tickets.consumed.v1",
            team.team_id,
            team.version,
            json!({
                "proposal_id": proposal.proposal_id,
                "team_id": team.team_id,
                "source_ticket_ids": &proposal.source_ticket_ids,
            }),
        )
        .await?;
    }
    let previous_proposal_version = proposal.version;
    proposal.status = TeamProposalStatus::Materialized;
    proposal.version += 1;
    proposal.updated_at = now;
    let updated = sqlx::query(
        "update hepta_team_proposals
         set status='materialized',version=$1,record_json=$2::jsonb,updated_at=$3
         where proposal_id=$4 and status='accepted' and version=$5",
    )
    .bind(
        i64::try_from(proposal.version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .bind(serde_json::to_value(&proposal).map_err(|error| {
        ApiError::internal(format!("encode materialized team proposal: {error}"))
    })?)
    .bind(now)
    .bind(proposal.proposal_id)
    .bind(
        i64::try_from(previous_proposal_version)
            .map_err(|_| ApiError::internal("proposal version overflow"))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "team_materialization_proposal_race",
            "accepted proposal changed while its team materialized",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &format!("{}:proposal-materialized", request.idempotency_key),
        "hepta.paper_raid.team_proposal.materialized.v1",
        proposal.proposal_id,
        proposal.version,
        json!({
            "proposal_id": proposal.proposal_id,
            "team_id": team.team_id,
            "status": proposal.status,
        }),
    )
    .await?;
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
    if let Some(paper) = paper {
        validate_paper_role_resources(paper)?;
    }
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
            player_phase: player_phase_semantic(paper.phase).to_string(),
            outcome: paper.outcome,
            outcome_reason: paper.outcome_reason.clone(),
            terminal_at: paper.terminal_at,
            role_resources: paper.role_resources.clone(),
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
    ensure_player_automatic_challenge_expiries_materialized(&state, &assertion, Utc::now()).await?;
    let mut raids = match &state.pool {
        None => {
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
        }
        Some(pool) => {
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
        }
    };
    raids.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.team_id.cmp(&right.team_id))
    });
    let current_raid = current_raid_from_sorted_history(&raids);
    Ok(Json(PlayerRaidState {
        schema: "hepta.paper_raid.player_raid_state.v1".into(),
        player_id: assertion.player_id,
        current_raid,
        raids,
    }))
}

fn current_raid_from_sorted_history(raids: &[PlayerRaidSummary]) -> Option<PlayerRaidSummary> {
    raids
        .iter()
        .find(|raid| {
            raid.team_status != TeamStatus::Archived
                && raid.paper.as_ref().is_none_or(|paper| {
                    matches!(
                        paper.outcome,
                        PaperChallengeOutcomeV1::InProgress
                            | PaperChallengeOutcomeV1::SubmissionReady
                    )
                })
        })
        .cloned()
}

fn validate_materialization_matchmaking_source(
    proposal: &TeamProposal,
    ordered_tickets: &[MatchmakingTicket],
) -> Result<Vec<String>, ApiError> {
    if proposal.member_player_ids.len() != 3
        || proposal.source_ticket_ids.len() != 3
        || ordered_tickets.len() != 3
        || ordered_tickets
            .iter()
            .map(|ticket| ticket.ticket_id)
            .collect::<HashSet<_>>()
            .len()
            != 3
        || ordered_tickets
            .iter()
            .map(|ticket| ticket.player_id)
            .collect::<HashSet<_>>()
            .len()
            != 3
        || ordered_tickets.iter().enumerate().any(|(index, ticket)| {
            ticket.challenge_id != proposal.challenge_id
                || ticket.requested_team_size != 3
                || ticket.player_id != proposal.member_player_ids[index]
                || ticket.ticket_id != proposal.source_ticket_ids[index]
                || ticket.matched_proposal_id != Some(proposal.proposal_id)
                || !matchmaking_partition_compatible(&ordered_tickets[0], ticket)
        })
    {
        return Err(ApiError::conflict(
            "team_proposal_provenance_mismatch",
            "team proposal source tickets no longer form its exact player, challenge, and private-party partition",
        ));
    }
    let assigned_roles = distinct_role_assignment(ordered_tickets).ok_or_else(|| {
        ApiError::conflict(
            "team_roles_not_complementary",
            "first-playable materialization requires three distinct compatible roles",
        )
    })?;
    let exact_preferences = frozen_team_proposal_preferences(ordered_tickets);
    let exact_assignments = frozen_team_proposal_role_assignments(ordered_tickets, &assigned_roles);
    if proposal.solver_version.as_deref() != Some(MATCHMAKING_SOLVER_VERSION_V2)
        || proposal.source_preferences != exact_preferences
        || proposal.role_assignments != exact_assignments
    {
        return Err(ApiError::conflict(
            "team_proposal_provenance_mismatch",
            "team proposal is missing or disagrees with its frozen solver, ordered preferences, or player-role assignment",
        ));
    }
    let ticket_source_versions = ordered_tickets
        .iter()
        .map(|ticket| {
            let transition_count = match ticket.status {
                MatchmakingTicketStatus::Matched => 1,
                MatchmakingTicketStatus::Consumed => 2,
                _ => {
                    return Err(ApiError::conflict(
                        "team_proposal_not_materializable",
                        "team proposal source ticket is not in its exact matched epoch",
                    ));
                }
            };
            ticket
                .version
                .checked_sub(transition_count)
                .map(|source_version| (ticket, source_version))
                .ok_or_else(|| {
                    ApiError::conflict(
                        "team_proposal_provenance_mismatch",
                        "team proposal source ticket version predates its required match transition",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected_match_key = deterministic_match_key_for_ticket_epochs(
        proposal.challenge_id,
        &ticket_source_versions,
        &ordered_tickets[0],
        &assigned_roles,
    );
    let exact_v2 = proposal.deterministic_match_key == expected_match_key
        && proposal.proposal_id == deterministic_uuid(&expected_match_key);
    if !exact_v2 {
        return Err(ApiError::conflict(
            "team_proposal_provenance_mismatch",
            "team proposal deterministic identity does not match its exact source ticket epochs",
        ));
    }
    Ok(assigned_roles)
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
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let now = Utc::now();
        let reserved_team_ids = memory.teams.keys().copied().collect::<HashSet<_>>();
        let eligible_player_ids = matchmaking_eligible_player_ids(&memory);
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::forbidden("human_player_not_registered", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if !eligible_player_ids.contains(&assertion.player_id) {
            return Err(ApiError::conflict(
                "matchmaking_player_not_ready",
                "team proposal decisions require an active player with exactly one active Agent binding",
            ));
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
        let expiry_sweep = expire_due_team_proposals_memory(
            &mut memory.collaboration,
            &reserved_team_ids,
            &eligible_player_ids,
            snapshot.challenge_id,
            now,
        )?;
        let current_expired = expiry_sweep
            .expired
            .iter()
            .any(|expired| expired.proposal_id == proposal_id);
        push_team_proposal_expiry_sweep_memory(&mut memory, &expiry_sweep)?;
        if !expiry_sweep.expired.is_empty() || !expiry_sweep.replacements.is_empty() {
            *memory_guard = memory.clone();
        }
        if current_expired {
            *memory_guard = memory;
            return Err(ApiError::conflict(
                "team_proposal_expired",
                "team proposal expired because its response window elapsed or a member lost matchmaking eligibility; eligible accepted members were released",
            ));
        }
        let snapshot = memory
            .collaboration
            .team_proposals
            .get(&proposal_id)
            .cloned()
            .expect("visible proposal remains stored after deadline sweep");
        if snapshot.status == TeamProposalStatus::Expired {
            return Err(ApiError::conflict(
                "team_proposal_expired",
                "team proposal expired because its response window elapsed or a member lost matchmaking eligibility; eligible accepted members were released",
            ));
        }
        if snapshot.status != TeamProposalStatus::Proposed {
            return Err(ApiError::conflict(
                "team_proposal_not_open",
                "team proposal is no longer awaiting member decisions",
            ));
        }
        let source_tickets = snapshot
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
        if let Err(error) = validate_materialization_matchmaking_source(&snapshot, &source_tickets)
        {
            let invalidation = invalidate_team_proposal_provenance_memory(
                &mut memory.collaboration,
                &reserved_team_ids,
                &eligible_player_ids,
                proposal_id,
                now,
            )?;
            push_team_proposal_expiry_sweep_memory(&mut memory, &invalidation)?;
            *memory_guard = memory;
            return Err(error);
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
        let mut replacement_proposals = Vec::new();
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
                    } else if matchmaking_ticket_deadline(ticket) <= now
                        || !eligible_player_ids.contains(&ticket.player_id)
                    {
                        MatchmakingTicketStatus::Expired
                    } else {
                        MatchmakingTicketStatus::Queued
                    };
                    ticket.matched_proposal_id = None;
                    ticket.version += 1;
                    ticket.updated_at = now;
                    ticket.queue_hint = None;
                }
                replacement_proposals = auto_match_all_queued_tickets_memory(
                    &mut memory.collaboration,
                    &reserved_team_ids,
                    &eligible_player_ids,
                    proposal.challenge_id,
                    now,
                )?;
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
        push_automatic_team_proposal_events_memory(&mut memory, &replacement_proposals)?;
        let replacement_proposal = replacement_proposals.first().cloned();
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
    let player_row = sqlx::query(
        "select player.record_json,
                (select count(*) from hepta_agent_bindings binding
                 where binding.player_id=player.player_id and binding.status='active')
                    as active_binding_count
         from hepta_human_players player where player.player_id=$1 for share of player",
    )
    .bind(assertion.player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden("human_player_not_registered", "human player does not exist")
    })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    if player.status != HumanPlayerStatus::Active
        || player_row.get::<i64, _>("active_binding_count") != 1
    {
        return Err(ApiError::conflict(
            "matchmaking_player_not_ready",
            "team proposal decisions require an active player with exactly one active Agent binding",
        ));
    }
    let proposal_snapshot_row =
        sqlx::query("select record_json from hepta_team_proposals where proposal_id=$1")
            .bind(proposal_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("team_proposal_not_found", "team proposal does not exist")
            })?;
    let proposal_snapshot: TeamProposal =
        decode_record(proposal_snapshot_row.get("record_json"), "team proposal")?;
    if !proposal_snapshot
        .member_player_ids
        .contains(&assertion.player_id)
    {
        return Err(ApiError::forbidden(
            "not_team_proposal_member",
            "only a proposed member can decide this team proposal",
        ));
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-paper-raid-matchmaking:{}",
            proposal_snapshot.challenge_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let now = postgres_transaction_now(&mut tx).await?;
    let expired =
        expire_due_team_proposals_postgres(&mut tx, proposal_snapshot.challenge_id, now).await?;
    if expired
        .iter()
        .any(|proposal| proposal.proposal_id == proposal_id)
    {
        tx.commit().await.map_err(ApiError::database)?;
        return Err(ApiError::conflict(
            "team_proposal_expired",
            "team proposal expired because its response window elapsed or a member lost matchmaking eligibility; eligible accepted members were released",
        ));
    }
    let proposal_row =
        sqlx::query("select record_json from hepta_team_proposals where proposal_id=$1 for update")
            .bind(proposal_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let mut proposal: TeamProposal =
        decode_record(proposal_row.get("record_json"), "team proposal")?;
    if proposal.status == TeamProposalStatus::Expired {
        return Err(ApiError::conflict(
            "team_proposal_expired",
            "team proposal expired because its response window elapsed or a member lost matchmaking eligibility; eligible accepted members were released",
        ));
    }
    if proposal.status != TeamProposalStatus::Proposed {
        return Err(ApiError::conflict(
            "team_proposal_not_open",
            "team proposal is no longer awaiting member decisions",
        ));
    }
    let source_rows = sqlx::query(
        "select version,record_json from hepta_matchmaking_tickets
         where ticket_id=any($1) order by ticket_id for update",
    )
    .bind(&proposal.source_ticket_ids)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let decoded_source_tickets = source_rows
        .into_iter()
        .map(|row| {
            let version = u64::try_from(row.get::<i64, _>("version"))
                .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
            let ticket: MatchmakingTicket =
                decode_record(row.get("record_json"), "matchmaking ticket")?;
            if ticket.version != version {
                return Err(ApiError::internal(
                    "matchmaking ticket record/version projection diverged",
                ));
            }
            Ok(ticket)
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let source_tickets = proposal
        .source_ticket_ids
        .iter()
        .map(|ticket_id| {
            decoded_source_tickets
                .iter()
                .find(|ticket| ticket.ticket_id == *ticket_id)
                .cloned()
                .ok_or_else(|| ApiError::internal("team proposal source ticket is missing"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Err(error) = validate_materialization_matchmaking_source(&proposal, &source_tickets) {
        invalidate_team_proposal_provenance_postgres(&mut tx, &proposal, now).await?;
        tx.commit().await.map_err(ApiError::database)?;
        return Err(error);
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
    let mut replacement_proposals = Vec::new();
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
            let rows = sqlx::query(
                "select ticket.version,ticket.record_json,player.status as player_status,
                        (select count(*) from hepta_agent_bindings binding
                         where binding.player_id=ticket.player_id and binding.status='active')
                            as active_binding_count
                 from hepta_matchmaking_tickets ticket
                 join hepta_human_players player on player.player_id=ticket.player_id
                 where ticket.ticket_id=any($1) order by ticket.ticket_id
                 for update of ticket for key share of player",
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
                let previous_version = u64::try_from(row.get::<i64, _>("version"))
                    .map_err(|_| ApiError::internal("matchmaking ticket version is invalid"))?;
                let mut ticket: MatchmakingTicket =
                    decode_record(row.get("record_json"), "matchmaking ticket")?;
                if ticket.version != previous_version
                    || ticket.status != MatchmakingTicketStatus::Matched
                    || ticket.matched_proposal_id != Some(proposal.proposal_id)
                {
                    return Err(ApiError::internal(
                        "team proposal source ticket projection diverged",
                    ));
                }
                ticket.status = if ticket.player_id == assertion.player_id {
                    MatchmakingTicketStatus::Cancelled
                } else if matchmaking_ticket_deadline(&ticket) <= now
                    || row.get::<String, _>("player_status") != "active"
                    || row.get::<i64, _>("active_binding_count") != 1
                {
                    MatchmakingTicketStatus::Expired
                } else {
                    MatchmakingTicketStatus::Queued
                };
                ticket.matched_proposal_id = None;
                ticket.version += 1;
                ticket.updated_at = now;
                ticket.queue_hint = None;
                let updated = sqlx::query(
                    "update hepta_matchmaking_tickets set status=$1,matched_proposal_id=null,
                     version=$2,record_json=$3::jsonb,updated_at=$4
                     where ticket_id=$5 and status='matched' and matched_proposal_id=$6
                       and version=$7",
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
                .bind(proposal.proposal_id)
                .bind(
                    i64::try_from(previous_version)
                        .map_err(|_| ApiError::internal("ticket version overflow"))?,
                )
                .execute(&mut *tx)
                .await
                .map_err(ApiError::database)?;
                if updated.rows_affected() != 1 {
                    return Err(ApiError::conflict(
                        "matchmaking_queue_changed",
                        "team proposal source ticket changed during decline",
                    ));
                }
            }
            replacement_proposals =
                auto_match_all_queued_tickets_postgres(&mut tx, proposal.challenge_id, now).await?;
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
    let replacement_proposal = replacement_proposals.first().cloned();
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

fn review_ready_source_manifest<'a>(
    manifests: &'a [ArtifactManifest],
    pin: &ArtifactManifestPinV1,
    paper_id: Uuid,
    challenge_id: Uuid,
    source_name: &'static str,
) -> Result<&'a ArtifactManifest, ApiError> {
    validate_digest_v2("review_ready_source_manifest_hash", &pin.manifest_hash)?;
    let matches = manifests
        .iter()
        .filter(|manifest| manifest.manifest_id == pin.manifest_id)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(ApiError::conflict(
            "review_ready_source_manifest_unavailable",
            format!("{source_name} must resolve to exactly one registered ArtifactManifest"),
        ));
    }
    let manifest = matches[0];
    if manifest.manifest_hash != pin.manifest_hash {
        return Err(ApiError::conflict(
            "review_ready_source_manifest_stale",
            format!("{source_name} manifest hash no longer matches its exact registered pin"),
        ));
    }
    if manifest.paper_project_id != paper_id {
        return Err(ApiError::forbidden(
            "review_ready_source_manifest_cross_paper",
            format!("{source_name} belongs to a different Paper"),
        ));
    }
    validate_artifact_challenge_id(&manifest.source_challenge_id, challenge_id)?;
    if manifest.binding_schema != ARTIFACT_MANIFEST_BINDING_SCHEMA_V1
        || manifest.review_ready_assembly.is_some()
    {
        return Err(ApiError::conflict(
            "review_ready_source_manifest_nested",
            format!("{source_name} must be one original registered source manifest"),
        ));
    }
    Ok(manifest)
}

fn validate_review_ready_source_roles(
    manifest: &ArtifactManifest,
    source_name: &'static str,
) -> Result<(), ApiError> {
    let mut role_counts = HashMap::<&str, usize>::new();
    for object in &manifest.objects {
        *role_counts.entry(object.role.as_str()).or_default() += 1;
        let media_allowed = match object.role.as_str() {
            "paper_source" => matches!(
                object.media_type.as_str(),
                "text/markdown; charset=utf-8" | "application/pdf"
            ),
            "bibliography" => matches!(
                object.media_type.as_str(),
                "application/x-bibtex" | "text/plain; charset=utf-8"
            ),
            "claim_evidence_graph" | "candidate" => object.media_type == "application/json",
            "frozen_evaluator" | "evaluator_support" => {
                object.media_type == "text/x-python; charset=utf-8"
            }
            "dataset" => matches!(
                object.media_type.as_str(),
                "application/json" | "text/csv; charset=utf-8"
            ),
            _ => false,
        };
        if !media_allowed {
            return Err(ApiError::conflict(
                "review_ready_source_role_or_media_mismatch",
                format!(
                    "{source_name} contains an unsupported role/media pair: {}/{}",
                    object.role, object.media_type
                ),
            ));
        }
    }
    let exact = |role: &str, count: usize| role_counts.get(role).copied().unwrap_or(0) == count;
    let valid = match source_name {
        "draft" => {
            manifest.objects.len() == 3
                && exact("paper_source", 1)
                && exact("bibliography", 1)
                && exact("claim_evidence_graph", 1)
        }
        "frozen_evaluator" => {
            exact("frozen_evaluator", 1)
                && manifest.objects.iter().all(|object| {
                    matches!(
                        object.role.as_str(),
                        "frozen_evaluator" | "evaluator_support"
                    )
                })
        }
        "dataset" => manifest.objects.len() == 1 && exact("dataset", 1),
        "candidate" => manifest.objects.len() == 1 && exact("candidate", 1),
        _ => false,
    };
    if !valid {
        return Err(ApiError::conflict(
            "review_ready_source_role_coverage_invalid",
            format!("{source_name} does not have its exact required role coverage"),
        ));
    }
    if manifest.objects.iter().any(|object| object.size == 0) {
        return Err(ApiError::conflict(
            "review_ready_source_object_empty",
            format!("{source_name} contains an empty executable or scientific object"),
        ));
    }
    Ok(())
}

type ExpectedReviewReadyContents = (
    Vec<NeutralArtifactObjectV1>,
    Vec<ArtifactReference>,
    Vec<String>,
);

fn expected_review_ready_contents(
    request: &CreateArtifactManifestRequest,
    paper_id: Uuid,
    challenge_id: Uuid,
    manifests: &[ArtifactManifest],
) -> Result<ExpectedReviewReadyContents, ApiError> {
    let assembly = request.review_ready_assembly.as_ref().ok_or_else(|| {
        ApiError::conflict(
            "review_ready_assembly_missing",
            "review-ready validation requires immutable source-manifest pins",
        )
    })?;
    if assembly.schema != REVIEW_READY_ARTIFACT_ASSEMBLY_SCHEMA_V1 {
        return Err(ApiError::bad_request(
            "review_ready_assembly_schema_mismatch",
            "review-ready assembly must use the exact supported schema",
        ));
    }
    let pins = [
        (&assembly.draft, "draft"),
        (&assembly.frozen_evaluator, "frozen_evaluator"),
        (&assembly.dataset, "dataset"),
        (&assembly.candidate, "candidate"),
    ];
    if pins
        .iter()
        .any(|(pin, _)| pin.manifest_id == request.manifest_id)
        || pins
            .iter()
            .map(|(pin, _)| pin.manifest_id)
            .collect::<HashSet<_>>()
            .len()
            != pins.len()
        || pins
            .iter()
            .map(|(pin, _)| pin.manifest_hash.as_str())
            .collect::<HashSet<_>>()
            .len()
            != pins.len()
    {
        return Err(ApiError::conflict(
            "review_ready_source_manifest_reused",
            "review-ready assembly requires four distinct source manifests and may not self-reference",
        ));
    }

    let mut expected_objects = Vec::new();
    let mut expected_locations = Vec::new();
    let mut required_run_ids = HashSet::new();
    for (pin, source_name) in pins {
        let source =
            review_ready_source_manifest(manifests, pin, paper_id, challenge_id, source_name)?;
        validate_review_ready_source_roles(source, source_name)?;
        let locations = source
            .storage_locations
            .iter()
            .map(|location| (location.logical_path.as_str(), location))
            .collect::<HashMap<_, _>>();
        if locations.len() != source.storage_locations.len()
            || source.objects.len() != source.storage_locations.len()
        {
            return Err(ApiError::conflict(
                "review_ready_source_storage_invalid",
                format!("{source_name} does not have one exact CAS location per object"),
            ));
        }
        for object in &source.objects {
            let location = locations.get(object.logical_path.as_str()).ok_or_else(|| {
                ApiError::conflict(
                    "review_ready_source_storage_invalid",
                    format!("{source_name} has an object without an exact CAS location"),
                )
            })?;
            if location.sha256 != object.sha256
                || location.uri != format!("cas://sha256/{}", object.sha256)
                || location.acl != ArtifactAcl::Team
            {
                return Err(ApiError::conflict(
                    "review_ready_source_storage_invalid",
                    format!("{source_name} CAS provenance is not exact and Paper-scoped"),
                ));
            }
            expected_objects.push(object.clone());
            expected_locations.push(ArtifactReference {
                logical_path: location.logical_path.clone(),
                sha256: location.sha256.clone(),
                uri: location.uri.clone(),
                acl: ArtifactAcl::Reviewers,
            });
        }
        required_run_ids.extend(source.required_run_ids.iter().cloned());
    }
    expected_objects.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    expected_locations.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
    let mut required_run_ids = required_run_ids.into_iter().collect::<Vec<_>>();
    required_run_ids.sort();
    if expected_objects
        .windows(2)
        .any(|pair| pair[0].logical_path == pair[1].logical_path)
        || expected_objects
            .iter()
            .map(|object| object.sha256.as_str())
            .collect::<HashSet<_>>()
            .len()
            != expected_objects.len()
    {
        return Err(ApiError::conflict(
            "review_ready_source_object_reused",
            "review-ready source objects must have distinct logical paths and distinct digests",
        ));
    }
    Ok((expected_objects, expected_locations, required_run_ids))
}

fn validate_review_ready_artifact_assembly(
    request: &CreateArtifactManifestRequest,
    manifest: &ArtifactManifest,
    paper_id: Uuid,
    challenge_id: Uuid,
    manifests: &[ArtifactManifest],
) -> Result<(), ApiError> {
    let (expected_objects, expected_locations, expected_run_ids) =
        expected_review_ready_contents(request, paper_id, challenge_id, manifests)?;
    if request.source_bundle.bundle_id != format!("review-ready-{}", request.manifest_id)
        || request.source_bundle.objects != expected_objects
        || request.storage_locations != expected_locations
        || request.source_bundle.required_run_ids != expected_run_ids
        || request.source_bundle.object_count
            != u64::try_from(expected_objects.len()).expect("bounded review-ready object count")
        || manifest.review_ready_assembly != request.review_ready_assembly
    {
        return Err(ApiError::conflict(
            "review_ready_assembly_projection_mismatch",
            "the proposed review-ready manifest is not the exact canonical projection of its registered same-Paper sources",
        ));
    }
    validate_review_ready_artifact_manifest(manifest)
}

pub(super) fn validate_review_ready_artifact_manifest(
    manifest: &ArtifactManifest,
) -> Result<(), ApiError> {
    if manifest.binding_schema != REVIEW_READY_ARTIFACT_MANIFEST_BINDING_SCHEMA_V1
        || manifest.review_ready_assembly.is_none()
    {
        return Err(ApiError::conflict(
            "review_ready_artifact_manifest_required",
            "Review claim requires a server-validated review-ready release ArtifactManifest",
        ));
    }
    let role_count = |role: &str| {
        manifest
            .objects
            .iter()
            .filter(|object| object.role == role)
            .count()
    };
    if role_count("paper_source") != 1
        || role_count("bibliography") != 1
        || role_count("claim_evidence_graph") != 1
        || role_count("frozen_evaluator") != 1
        || role_count("dataset") != 1
        || role_count("candidate") != 1
        || manifest.objects.iter().any(|object| {
            !matches!(
                object.role.as_str(),
                "paper_source"
                    | "bibliography"
                    | "claim_evidence_graph"
                    | "frozen_evaluator"
                    | "evaluator_support"
                    | "dataset"
                    | "candidate"
            )
        })
    {
        return Err(ApiError::conflict(
            "review_ready_artifact_role_coverage_invalid",
            "Review claim requires exact human-paper, evaluator, dataset, and candidate role coverage",
        ));
    }
    let locations = manifest
        .storage_locations
        .iter()
        .map(|location| (location.logical_path.as_str(), location))
        .collect::<HashMap<_, _>>();
    if locations.len() != manifest.objects.len()
        || manifest.objects.len() != manifest.storage_locations.len()
        || manifest
            .objects
            .iter()
            .map(|object| object.sha256.as_str())
            .collect::<HashSet<_>>()
            .len()
            != manifest.objects.len()
        || manifest.objects.iter().any(|object| {
            locations
                .get(object.logical_path.as_str())
                .is_none_or(|location| {
                    location.sha256 != object.sha256
                        || location.uri != format!("cas://sha256/{}", object.sha256)
                        || location.acl != ArtifactAcl::Reviewers
                })
        })
    {
        return Err(ApiError::conflict(
            "review_ready_artifact_storage_invalid",
            "Review claim requires one reviewer-readable Paper-scoped CAS key per distinct object",
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

fn current_merged_section_head_bindings(
    paper_id: Uuid,
    section_heads: &[SectionHead],
    section_merges: &[SectionMerge],
) -> Result<Vec<PaperRevisionSectionHeadBinding>, ApiError> {
    let paper_merges = section_merges
        .iter()
        .filter(|merge| merge.paper_project_id == paper_id)
        .collect::<Vec<_>>();
    let mut bindings = Vec::new();
    for head in section_heads
        .iter()
        .filter(|head| head.paper_project_id == paper_id)
    {
        if head.current_head_revision_id == head.base_paper_revision_id {
            continue;
        }
        if !paper_merges.iter().any(|merge| {
            merge.section_key == head.section_key
                && merge.merged_section_revision_id == head.current_head_revision_id
        }) {
            return Err(ApiError::internal(
                "advanced section head is not backed by its authoritative merge",
            ));
        }
        bindings.push(PaperRevisionSectionHeadBinding {
            section_key: head.section_key.clone(),
            base_paper_revision_id: head.base_paper_revision_id,
            current_head_revision_id: head.current_head_revision_id,
        });
    }
    bindings.sort();
    if bindings
        .windows(2)
        .any(|pair| pair[0].section_key == pair[1].section_key)
    {
        return Err(ApiError::internal(
            "paper has duplicate authoritative section heads",
        ));
    }
    if paper_merges.iter().any(|merge| {
        !bindings
            .iter()
            .any(|binding| binding.section_key == merge.section_key)
    }) {
        return Err(ApiError::internal(
            "section merge is missing its authoritative current head",
        ));
    }
    Ok(bindings)
}

fn revision_parent_chain_covers_section_bases(
    paper_id: Uuid,
    parent_revision_id: Option<Uuid>,
    paper_revisions: &[PaperRevision],
    section_head_bindings: &[PaperRevisionSectionHeadBinding],
) -> bool {
    if section_head_bindings.is_empty() {
        return true;
    }
    let revisions = paper_revisions
        .iter()
        .filter(|revision| revision.paper_project_id == paper_id)
        .map(|revision| (revision.revision_id, revision))
        .collect::<HashMap<_, _>>();
    let mut ancestors = HashSet::new();
    let mut cursor = parent_revision_id;
    while let Some(revision_id) = cursor {
        if !ancestors.insert(revision_id) {
            return false;
        }
        let Some(revision) = revisions.get(&revision_id) else {
            return false;
        };
        cursor = revision.parent_revision_id;
    }
    section_head_bindings
        .iter()
        .all(|binding| ancestors.contains(&binding.base_paper_revision_id))
}

#[cfg(test)]
fn bind_current_section_heads(
    mut binding: PaperRevisionArtifactBinding,
    parent_revision_id: Option<Uuid>,
    paper_revisions: &[PaperRevision],
    section_heads: &[SectionHead],
    section_merges: &[SectionMerge],
) -> Result<PaperRevisionArtifactBinding, ApiError> {
    let section_head_bindings = current_merged_section_head_bindings(
        binding.paper_project_id,
        section_heads,
        section_merges,
    )?;
    if !revision_parent_chain_covers_section_bases(
        binding.paper_project_id,
        parent_revision_id,
        paper_revisions,
        &section_head_bindings,
    ) {
        return Err(ApiError::conflict(
            "paper_revision_section_lineage_mismatch",
            "paper revision parent chain must inherit every merged section head's whole-paper base",
        ));
    }
    binding.section_head_bindings = section_head_bindings;
    Ok(binding)
}

pub(super) struct ResolvedPaperRevisionMaterialization {
    pub artifact_binding: PaperRevisionArtifactBinding,
    pub descriptor: SectionMaterializationDescriptorV1,
    pub root: String,
    pub section_head_count: usize,
}

fn capture_section_materialization(
    paper_id: Uuid,
    revision_id: Uuid,
    parent_revision_id: Option<Uuid>,
    paper_revisions: &[PaperRevision],
    section_heads: &[SectionHead],
    section_revisions: &[SectionRevision],
    section_merges: &[SectionMerge],
) -> Result<
    (
        SectionMaterializationDescriptorV1,
        String,
        Vec<PaperRevisionSectionHeadBinding>,
    ),
    ApiError,
> {
    let parent_materialization_root = match parent_revision_id {
        Some(parent_id) => {
            let parent = paper_revisions
                .iter()
                .find(|revision| {
                    revision.paper_project_id == paper_id && revision.revision_id == parent_id
                })
                .ok_or_else(|| ApiError::internal("paper revision parent is missing"))?;
            match (
                parent.section_materialization.as_ref(),
                parent.section_materialization_root.as_ref(),
            ) {
                (Some(descriptor), Some(root)) => {
                    let expected = section_materialization_root(descriptor).map_err(|error| {
                        ApiError::internal(format!(
                            "parent section materialization is invalid: {error}"
                        ))
                    })?;
                    if expected != *root {
                        return Err(ApiError::internal(
                            "parent section materialization root does not match its descriptor",
                        ));
                    }
                    Some(root.clone())
                }
                (None, None) => None,
                _ => {
                    return Err(ApiError::internal(
                        "parent section materialization descriptor/root pair is incomplete",
                    ))
                }
            }
        }
        None => None,
    };

    let mut entries = Vec::new();
    let mut legacy_bindings = Vec::new();
    for head in section_heads
        .iter()
        .filter(|head| head.paper_project_id == paper_id)
    {
        if head.current_head_revision_id == head.base_paper_revision_id {
            continue;
        }
        let matching_merges = section_merges
            .iter()
            .filter(|merge| {
                merge.paper_project_id == paper_id
                    && merge.section_key == head.section_key
                    && merge.merged_section_revision_id == head.current_head_revision_id
            })
            .collect::<Vec<_>>();
        if matching_merges.len() != 1 {
            return Err(ApiError::internal(
                "advanced section head must have exactly one authoritative merge",
            ));
        }
        let merge = matching_merges[0];
        let section_revision = section_revisions
            .iter()
            .find(|revision| {
                revision.paper_project_id == paper_id
                    && revision.section_revision_id == head.current_head_revision_id
            })
            .ok_or_else(|| ApiError::internal("merged section revision is missing"))?;
        if section_revision.section_key != head.section_key
            || section_revision.status != SectionRevisionStatus::Merged
            || merge.section_revision_id != section_revision.section_revision_id
            || merge.parent_revision_id != section_revision.parent_revision_id
        {
            return Err(ApiError::internal(
                "section head, merge, and patch revision lineage disagree",
            ));
        }
        entries.push(SectionMaterializationEntryV1 {
            section_key: head.section_key.clone(),
            base_paper_revision_id: head.base_paper_revision_id,
            head_section_revision_id: head.current_head_revision_id,
            merge_id: merge.merge_id,
            patch_manifest_id: section_revision.patch_manifest_id,
            patch_hash: section_revision.patch_hash.clone(),
        });
        legacy_bindings.push(PaperRevisionSectionHeadBinding {
            section_key: head.section_key.clone(),
            base_paper_revision_id: head.base_paper_revision_id,
            current_head_revision_id: head.current_head_revision_id,
        });
    }
    entries.sort();
    legacy_bindings.sort();
    if !revision_parent_chain_covers_section_bases(
        paper_id,
        parent_revision_id,
        paper_revisions,
        &legacy_bindings,
    ) {
        return Err(ApiError::conflict(
            "paper_revision_section_lineage_mismatch",
            "paper revision parent chain must inherit every merged section head's whole-paper base",
        ));
    }
    let descriptor = SectionMaterializationDescriptorV1 {
        schema: SECTION_MATERIALIZATION_V1.to_string(),
        paper_project_id: paper_id,
        revision_id,
        parent_revision_id,
        parent_materialization_root,
        sections: entries,
    };
    let root = section_materialization_root(&descriptor).map_err(|error| {
        ApiError::internal(format!("canonicalize section materialization: {error}"))
    })?;
    Ok((descriptor, root, legacy_bindings))
}

fn current_revision_materialization_is_complete(
    paper: &PaperProject,
    paper_revisions: &[PaperRevision],
    revision_artifact_bindings: &[PaperRevisionArtifactBinding],
    section_heads: &[SectionHead],
    section_revisions: &[SectionRevision],
    section_merges: &[SectionMerge],
) -> bool {
    let Some(current_revision_id) = paper.current_revision_id else {
        return false;
    };
    let Some(revision) = paper_revisions.iter().find(|revision| {
        revision.paper_project_id == paper.paper_project_id
            && revision.revision_id == current_revision_id
    }) else {
        return false;
    };
    let (Some(descriptor), Some(root)) = (
        revision.section_materialization.as_ref(),
        revision.section_materialization_root.as_ref(),
    ) else {
        return false;
    };
    if descriptor.paper_project_id != paper.paper_project_id
        || descriptor.revision_id != current_revision_id
        || descriptor.parent_revision_id != revision.parent_revision_id
        || section_materialization_root(descriptor).ok().as_ref() != Some(root)
    {
        return false;
    }
    if let Some(parent_id) = revision.parent_revision_id {
        let Some(parent) = paper_revisions.iter().find(|candidate| {
            candidate.paper_project_id == paper.paper_project_id
                && candidate.revision_id == parent_id
        }) else {
            return false;
        };
        if descriptor.parent_materialization_root != parent.section_materialization_root {
            return false;
        }
    } else if descriptor.parent_materialization_root.is_some() {
        return false;
    }
    if section_heads
        .iter()
        .filter(|head| head.paper_project_id == paper.paper_project_id)
        .any(|head| {
            head.base_paper_revision_id != current_revision_id
                || head.current_head_revision_id != current_revision_id
        })
    {
        return false;
    }
    for entry in &descriptor.sections {
        let Some(merge) = section_merges.iter().find(|merge| {
            merge.paper_project_id == paper.paper_project_id && merge.merge_id == entry.merge_id
        }) else {
            return false;
        };
        let Some(section_revision) = section_revisions.iter().find(|section_revision| {
            section_revision.paper_project_id == paper.paper_project_id
                && section_revision.section_revision_id == entry.head_section_revision_id
        }) else {
            return false;
        };
        if merge.section_key != entry.section_key
            || merge.merged_section_revision_id != entry.head_section_revision_id
            || section_revision.section_key != entry.section_key
            || section_revision.patch_manifest_id != entry.patch_manifest_id
            || section_revision.patch_hash != entry.patch_hash
        {
            return false;
        }
    }
    let expected_legacy_bindings = descriptor
        .sections
        .iter()
        .map(|entry| PaperRevisionSectionHeadBinding {
            section_key: entry.section_key.clone(),
            base_paper_revision_id: entry.base_paper_revision_id,
            current_head_revision_id: entry.head_section_revision_id,
        })
        .collect::<Vec<_>>();
    revision_artifact_bindings.iter().any(|binding| {
        binding.paper_project_id == paper.paper_project_id
            && binding.revision_id == current_revision_id
            && binding.section_head_bindings == expected_legacy_bindings
    })
}

pub(super) fn paper_revision_covers_section_merges(
    paper: &PaperProject,
    paper_revisions: &[PaperRevision],
    revision_artifact_bindings: &[PaperRevisionArtifactBinding],
    section_heads: &[SectionHead],
    section_revisions: &[SectionRevision],
    section_merges: &[SectionMerge],
) -> bool {
    if current_revision_materialization_is_complete(
        paper,
        paper_revisions,
        revision_artifact_bindings,
        section_heads,
        section_revisions,
        section_merges,
    ) {
        return true;
    }
    // Historical records created before section-materialization roots remain
    // readable and keep their exact v2 release hashes. They use the original
    // sidecar gate; every newly-created revision takes the rooted path above.
    let Some(current_revision_id) = paper.current_revision_id else {
        return false;
    };
    let Some(current_revision) = paper_revisions.iter().find(|revision| {
        revision.paper_project_id == paper.paper_project_id
            && revision.revision_id == current_revision_id
    }) else {
        return false;
    };
    if current_revision.section_materialization.is_some()
        || current_revision.section_materialization_root.is_some()
    {
        return false;
    }
    let Ok(current_heads) =
        current_merged_section_head_bindings(paper.paper_project_id, section_heads, section_merges)
    else {
        return false;
    };
    if current_heads.is_empty()
        || !revision_parent_chain_covers_section_bases(
            paper.paper_project_id,
            current_revision.parent_revision_id,
            paper_revisions,
            &current_heads,
        )
    {
        return false;
    }
    revision_artifact_bindings.iter().any(|binding| {
        binding.paper_project_id == paper.paper_project_id
            && binding.revision_id == current_revision_id
            && binding.section_head_bindings == current_heads
    })
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
        section_head_bindings: Vec::new(),
        created_at: Utc::now(),
    })
}

pub(super) fn resolve_revision_artifact_binding_memory(
    memory: &PaperRaidMemory,
    paper_id: Uuid,
    request: &CreatePaperRevisionRequest,
) -> Result<ResolvedPaperRevisionMaterialization, ApiError> {
    let now = Utc::now();
    let active_lease = memory.collaboration.leases.values().any(|lease| {
        lease.paper_project_id == paper_id
            && lease.status == SectionLeaseStatus::Active
            && lease.expires_at > now
    });
    let unfinished_revision = memory
        .collaboration
        .section_revisions
        .values()
        .any(|revision| {
            revision.paper_project_id == paper_id
                && matches!(
                    &revision.status,
                    SectionRevisionStatus::Proposed | SectionRevisionStatus::Approved
                )
        });
    let unfinished_proposal = memory.collaboration.proposals.values().any(|proposal| {
        proposal.paper_project_id == paper_id
            && (matches!(&proposal.status, AgentProposalStatus::Submitted)
                || (proposal.status == AgentProposalStatus::Accepted
                    && proposal.proposal_kind == AgentProposalKind::Delivery
                    && !memory
                        .collaboration
                        .section_revisions
                        .values()
                        .any(|revision| revision.proposal_id == proposal.proposal_id)))
    });
    if active_lease || unfinished_revision || unfinished_proposal {
        return Err(ApiError::conflict(
            "section_materialization_in_flight",
            "whole-paper materialization requires every section lease, proposal, and revision to be terminal",
        ));
    }
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
    let binding =
        resolve_paper_revision_artifact_binding(request.revision_id, paper_id, manifest, request)?;
    let paper_revisions = memory
        .revisions
        .values()
        .filter(|revision| revision.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_heads = memory
        .collaboration
        .section_heads
        .values()
        .filter(|head| head.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_merges = memory
        .collaboration
        .section_merges
        .values()
        .filter(|merge| merge.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_revisions = memory
        .collaboration
        .section_revisions
        .values()
        .filter(|revision| revision.paper_project_id == paper_id)
        .cloned()
        .collect::<Vec<_>>();
    let (descriptor, root, section_head_bindings) = capture_section_materialization(
        paper_id,
        request.revision_id,
        request.parent_revision_id,
        &paper_revisions,
        &section_heads,
        &section_revisions,
        &section_merges,
    )?;
    let mut artifact_binding = binding;
    artifact_binding.section_head_bindings = section_head_bindings;
    Ok(ResolvedPaperRevisionMaterialization {
        artifact_binding,
        descriptor,
        root,
        section_head_count: section_heads.len(),
    })
}

pub(super) fn rebase_section_heads_memory(
    memory: &mut PaperRaidMemory,
    paper_id: Uuid,
    revision_id: Uuid,
    expected_head_count: usize,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let actual_head_count = memory
        .collaboration
        .section_heads
        .values()
        .filter(|head| head.paper_project_id == paper_id)
        .count();
    if actual_head_count != expected_head_count {
        return Err(ApiError::conflict(
            "section_materialization_head_set_changed",
            "section head set changed during whole-paper materialization",
        ));
    }
    for head in memory
        .collaboration
        .section_heads
        .values_mut()
        .filter(|head| head.paper_project_id == paper_id)
    {
        head.base_paper_revision_id = revision_id;
        head.current_head_revision_id = revision_id;
        head.version += 1;
        head.updated_at = now;
    }
    Ok(())
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

async fn load_paper_revision_lineage_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<PaperRevision>, ApiError> {
    sqlx::query(
        "select record_json from hepta_paper_revisions
         where paper_project_id=$1 order by revision_number,revision_id",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "paper revision"))
    .collect()
}

async fn load_section_heads_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<SectionHead>, ApiError> {
    sqlx::query(
        "select paper_project_id,section_key,base_paper_revision_id,current_head_revision_id,
                fencing_token,version,updated_at
         from hepta_section_heads where paper_project_id=$1 order by section_key",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
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
    .collect()
}

async fn load_section_merges_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<SectionMerge>, ApiError> {
    sqlx::query(
        "select record_json from hepta_section_merges
         where paper_project_id=$1 order by section_key,merged_at,merge_id",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "section merge"))
    .collect()
}

async fn load_section_revisions_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<SectionRevision>, ApiError> {
    sqlx::query(
        "select record_json from hepta_section_revisions
         where paper_project_id=$1 order by section_key,created_at,section_revision_id",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "section revision"))
    .collect()
}

async fn load_revision_artifact_bindings_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
) -> Result<Vec<PaperRevisionArtifactBinding>, ApiError> {
    sqlx::query(
        "select record_json from hepta_paper_revision_artifact_bindings
         where paper_project_id=$1 order by revision_id",
    )
    .bind(paper_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .into_iter()
    .map(|row| decode_record(row.get("record_json"), "paper revision artifact binding"))
    .collect()
}

pub(super) fn paper_revision_covers_section_merges_memory(
    memory: &PaperRaidMemory,
    paper: &PaperProject,
) -> bool {
    let paper_revisions = memory
        .revisions
        .values()
        .filter(|revision| revision.paper_project_id == paper.paper_project_id)
        .cloned()
        .collect::<Vec<_>>();
    let revision_artifact_bindings = memory
        .collaboration
        .revision_artifact_bindings
        .values()
        .filter(|binding| binding.paper_project_id == paper.paper_project_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_heads = memory
        .collaboration
        .section_heads
        .values()
        .filter(|head| head.paper_project_id == paper.paper_project_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_merges = memory
        .collaboration
        .section_merges
        .values()
        .filter(|merge| merge.paper_project_id == paper.paper_project_id)
        .cloned()
        .collect::<Vec<_>>();
    let section_revisions = memory
        .collaboration
        .section_revisions
        .values()
        .filter(|revision| revision.paper_project_id == paper.paper_project_id)
        .cloned()
        .collect::<Vec<_>>();
    paper_revision_covers_section_merges(
        paper,
        &paper_revisions,
        &revision_artifact_bindings,
        &section_heads,
        &section_revisions,
        &section_merges,
    )
}

pub(super) async fn paper_revision_covers_section_merges_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper: &PaperProject,
) -> Result<bool, ApiError> {
    let paper_revisions = load_paper_revision_lineage_postgres(tx, paper.paper_project_id).await?;
    let revision_artifact_bindings =
        load_revision_artifact_bindings_postgres(tx, paper.paper_project_id).await?;
    let section_heads = load_section_heads_postgres(tx, paper.paper_project_id).await?;
    let section_revisions = load_section_revisions_postgres(tx, paper.paper_project_id).await?;
    let section_merges = load_section_merges_postgres(tx, paper.paper_project_id).await?;
    Ok(paper_revision_covers_section_merges(
        paper,
        &paper_revisions,
        &revision_artifact_bindings,
        &section_heads,
        &section_revisions,
        &section_merges,
    ))
}

pub(super) async fn resolve_revision_artifact_binding_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    request: &CreatePaperRevisionRequest,
) -> Result<ResolvedPaperRevisionMaterialization, ApiError> {
    let in_flight = sqlx::query(
        "select
           exists(
             select 1 from hepta_section_leases
             where paper_project_id=$1 and status='active' and expires_at > now()
           ) as active_lease,
           exists(
             select 1 from hepta_section_revisions
             where paper_project_id=$1 and status in ('proposed','approved')
           ) as unfinished_revision,
           exists(
             select 1 from hepta_agent_proposals p
             where p.paper_project_id=$1 and (
               p.status='submitted'
               or (
                 p.status='accepted' and p.proposal_kind='delivery'
                 and not exists(
                   select 1 from hepta_section_revisions r
                   where r.paper_project_id=p.paper_project_id
                     and r.proposal_id=p.proposal_id
                 )
               )
             )
           ) as unfinished_proposal",
    )
    .bind(paper_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if in_flight.get::<bool, _>("active_lease")
        || in_flight.get::<bool, _>("unfinished_revision")
        || in_flight.get::<bool, _>("unfinished_proposal")
    {
        return Err(ApiError::conflict(
            "section_materialization_in_flight",
            "whole-paper materialization requires every section lease, proposal, and revision to be terminal",
        ));
    }
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
    let binding =
        resolve_paper_revision_artifact_binding(request.revision_id, paper_id, manifest, request)?;
    let paper_revisions = load_paper_revision_lineage_postgres(tx, paper_id).await?;
    let section_heads = load_section_heads_postgres(tx, paper_id).await?;
    let section_revisions = load_section_revisions_postgres(tx, paper_id).await?;
    let section_merges = load_section_merges_postgres(tx, paper_id).await?;
    let (descriptor, root, section_head_bindings) = capture_section_materialization(
        paper_id,
        request.revision_id,
        request.parent_revision_id,
        &paper_revisions,
        &section_heads,
        &section_revisions,
        &section_merges,
    )?;
    let mut artifact_binding = binding;
    artifact_binding.section_head_bindings = section_head_bindings;
    Ok(ResolvedPaperRevisionMaterialization {
        artifact_binding,
        descriptor,
        root,
        section_head_count: section_heads.len(),
    })
}

pub(super) async fn rebase_section_heads_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    revision_id: Uuid,
    expected_head_count: usize,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let updated = sqlx::query(
        "update hepta_section_heads
         set base_paper_revision_id=$1,current_head_revision_id=$1,
             version=version+1,updated_at=$2
         where paper_project_id=$3",
    )
    .bind(revision_id)
    .bind(now)
    .bind(paper_id)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected()
        != u64::try_from(expected_head_count)
            .map_err(|_| ApiError::internal("section head count overflow"))?
    {
        return Err(ApiError::conflict(
            "section_materialization_head_set_changed",
            "section head set changed during whole-paper materialization",
        ));
    }
    Ok(())
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
        binding_schema: if request.review_ready_assembly.is_some() {
            REVIEW_READY_ARTIFACT_MANIFEST_BINDING_SCHEMA_V1.to_string()
        } else {
            ARTIFACT_MANIFEST_BINDING_SCHEMA_V1.to_string()
        },
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
        review_ready_assembly: request.review_ready_assembly.clone(),
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
        if request.review_ready_assembly.is_some() {
            let source_manifests = memory
                .collaboration
                .artifact_manifests
                .values()
                .cloned()
                .collect::<Vec<_>>();
            validate_review_ready_artifact_assembly(
                &request,
                &manifest,
                paper_id,
                paper.challenge_id,
                &source_manifests,
            )?;
        }
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
    if let Some(assembly) = &request.review_ready_assembly {
        let source_ids = vec![
            assembly.draft.manifest_id,
            assembly.frozen_evaluator.manifest_id,
            assembly.dataset.manifest_id,
            assembly.candidate.manifest_id,
        ];
        let source_manifests = sqlx::query(
            "select record_json from hepta_artifact_manifests
             where paper_project_id=$1 and manifest_id=any($2::uuid[])
             order by manifest_id for share",
        )
        .bind(paper_id)
        .bind(&source_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "review-ready source manifest"))
        .collect::<Result<Vec<ArtifactManifest>, ApiError>>()?;
        validate_review_ready_artifact_assembly(
            &request,
            &manifest,
            paper_id,
            paper.challenge_id,
            &source_manifests,
        )?;
    }
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
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Evidence)?;
        require_author_role(
            &team,
            assertion.player_id,
            "evidence",
            "evidence verification",
        )?;
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
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Evidence)?;
    require_author_role(
        &team,
        assertion.player_id,
        "evidence",
        "evidence verification",
    )?;
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
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Citation)?;
        require_author_role(
            &team,
            assertion.player_id,
            "evidence",
            "citation verification",
        )?;
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
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Citation)?;
    require_author_role(
        &team,
        assertion.player_id,
        "evidence",
        "citation verification",
    )?;
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
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::ExperimentPlan)?;
        require_author_role(
            &team,
            assertion.player_id,
            "experiment",
            "experiment preregistration",
        )?;
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
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::ExperimentPlan)?;
    require_author_role(
        &team,
        assertion.player_id,
        "experiment",
        "experiment preregistration",
    )?;
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

fn run_record_from_request(
    paper_id: Uuid,
    request: &CreateRunRecordRequest,
    created_at: DateTime<Utc>,
) -> RunRecord {
    RunRecord {
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
        created_at,
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
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let (mut paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        let authority_now = Utc::now();
        require_collaboration_phase_at(&paper, CollaborationMutation::Run, authority_now)?;
        require_author_role(
            &team,
            assertion.player_id,
            "experiment",
            "experiment run recording",
        )?;
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
        let record = run_record_from_request(paper_id, &request, authority_now);
        let role_resource_action =
            apply_run_role_resources(&mut paper, assertion.player_id, &record, authority_now)?;
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
        if role_resource_action.is_some() {
            memory.papers.insert(paper_id, paper.clone());
        }
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.run_record.created.v1",
            paper_id,
            record.run_record_id,
            1,
            json!({
                "run_record_id":record.run_record_id,
                "status":record.status,
                "failure_retained":record.status == RunStatus::Failed && record.failure_hash.is_some(),
                "role_resource_action":role_resource_action,
                "ranking_eligible":false,
                "reward_eligible":false,
                "economic_eligibility":false,
            }),
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
    let (mut paper, team) =
        paper_and_team_for_role_resource_mutation_postgres(&mut tx, paper_id, &assertion).await?;
    let authority_now: DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    require_collaboration_phase_at(&paper, CollaborationMutation::Run, authority_now)?;
    require_author_role(
        &team,
        assertion.player_id,
        "experiment",
        "experiment run recording",
    )?;
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
    let previous_paper_version = paper.version;
    let record = run_record_from_request(paper_id, &request, authority_now);
    let role_resource_action =
        apply_run_role_resources(&mut paper, assertion.player_id, &record, authority_now)?;
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
    if role_resource_action.is_some() {
        persist_role_resource_paper_postgres(&mut tx, &paper, previous_paper_version).await?;
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.run_record.created.v1",
        paper_id,
        record.run_record_id,
        1,
        json!({
            "run_record_id":record.run_record_id,
            "status":record.status,
            "failure_retained":record.status == RunStatus::Failed && record.failure_hash.is_some(),
            "role_resource_action":role_resource_action,
            "ranking_eligible":false,
            "reward_eligible":false,
            "economic_eligibility":false,
        }),
    )
    .await?;
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
        let (paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        require_collaboration_phase(&paper, CollaborationMutation::Claim)?;
        require_author_role(
            &team,
            assertion.player_id,
            "evidence",
            "claim-to-evidence binding",
        )?;
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
    let (paper, team) = paper_and_team_postgres(&mut tx, paper_id, &assertion).await?;
    require_collaboration_phase(&paper, CollaborationMutation::Claim)?;
    require_author_role(
        &team,
        assertion.player_id,
        "evidence",
        "claim-to-evidence binding",
    )?;
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
    if state.pool.is_none() {
        let mut memory_guard = state.paper_raid.write().await;
        let mut memory = memory_guard.clone();
        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::seconds(i64::try_from(request.ttl_seconds).expect("ttl fits i64"));
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
    let now = postgres_transaction_now(&mut tx).await?;
    let expires_at =
        now + chrono::Duration::seconds(i64::try_from(request.ttl_seconds).expect("ttl fits i64"));
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
    if request.lease_fencing_token == 0
        || request.lease_fencing_token > JSON_SAFE_U64_MAX
        || request.expected_work_version == 0
        || request.expected_work_version > JSON_SAFE_U64_MAX
    {
        return Err(ApiError::bad_request(
            "invalid_agent_proposal_epoch",
            "lease_fencing_token and expected_work_version must be positive JSON-safe integers",
        ));
    }
    signed_time(request.signed_at_unix)?;
    Ok(())
}

fn verify_agent_proposal(
    paper_id: Uuid,
    request: &CreateAgentProposalRequest,
    artifact_manifest_hash: &str,
    binding: &AgentBinding,
) -> Result<String, ApiError> {
    if binding.status != AgentBindingStatus::Active
        || binding.binding_id != request.binding_id
        || binding.agent_id != request.agent_id
    {
        return Err(ApiError::forbidden(
            "agent_binding_not_active",
            "proposal Agent binding is missing, revoked, or belongs to another Agent",
        ));
    }
    let public_key = canonical_public_key(&binding.agent_public_key)?;
    let public_key_bytes = BASE64
        .decode(&public_key)
        .map_err(|_| ApiError::internal("canonical Agent public key decode failed"))?;
    let expected_key_id = crate::paper_raid_contracts::sha256_digest(&public_key_bytes);
    if binding.agent_key_id != expected_key_id
        || binding.agent_public_key_hash != expected_key_id
        || binding.agent_public_key != public_key
    {
        return Err(ApiError::internal(
            "active Agent binding key material is inconsistent",
        ));
    }
    if request.agent_key_id != binding.agent_key_id {
        return Err(ApiError::forbidden(
            "agent_key_not_current",
            "agent_key_id does not match the active Agent binding key",
        ));
    }
    let signing = AgentProposalSigningV2 {
        schema: AGENT_PROPOSAL_V2.to_string(),
        proposal_id: request.proposal_id,
        paper_project_id: paper_id,
        work_item_id: request.work_item_id,
        section_key: request.section_key.clone(),
        parent_revision_id: request.parent_revision_id,
        lease_id: request.lease_id,
        lease_fencing_token: request.lease_fencing_token,
        expected_work_version: request.expected_work_version,
        proposal_kind: request.proposal_kind.as_str().to_string(),
        payload_hash: request.payload_hash.clone(),
        artifact_manifest_id: request.artifact_manifest_id,
        artifact_manifest_hash: artifact_manifest_hash.to_string(),
        agent_id: request.agent_id.clone(),
        binding_id: request.binding_id,
        agent_key_id: request.agent_key_id.clone(),
        signed_at_unix: request.signed_at_unix,
    };
    let signing_bytes = agent_proposal_v2_signing_bytes(&signing)
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
) -> Result<(WorkItem, ArtifactManifest, AgentBinding, SectionLease), ApiError> {
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
    if !matches!(
        work.status,
        WorkItemStatus::Planned | WorkItemStatus::InProgress | WorkItemStatus::Review
    ) {
        return Err(ApiError::conflict(
            "agent_proposal_work_not_active",
            "Agent proposals require work in planned, in_progress, or review status",
        ));
    }
    if work.version != request.expected_work_version {
        return Err(version_conflict(
            "Agent proposal work item",
            request.expected_work_version,
            work.version,
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
    let active_leases = memory
        .collaboration
        .leases
        .values()
        .filter(|lease| {
            lease.paper_project_id == paper.paper_project_id
                && lease.section_key == request.section_key
                && lease.status == SectionLeaseStatus::Active
        })
        .collect::<Vec<_>>();
    let lease = match active_leases.as_slice() {
        [lease] => *lease,
        _ => {
            return Err(ApiError::conflict(
                "agent_proposal_requires_current_lease",
                "Agent proposals require exactly one current active section lease",
            ));
        }
    };
    if lease.lease_id != request.lease_id
        || lease.fencing_token != request.lease_fencing_token
        || lease.fencing_token != head.fencing_token
    {
        return Err(ApiError::conflict(
            "stale_agent_proposal_lease_epoch",
            "Agent proposal lease identity or fencing token is not the current section epoch",
        ));
    }
    if lease.holder_player_id != binding.player_id || lease.holder_binding_id != request.binding_id
    {
        return Err(ApiError::conflict(
            "agent_proposal_requires_current_lease",
            "Agent proposal binding must hold the current unexpired section lease and fencing token",
        ));
    }
    Ok((work, manifest, binding, (*lease).clone()))
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
        let (_, manifest, binding, lease) =
            validate_agent_scope_memory(&memory, &paper, &team, &request)?;
        let public_key =
            verify_agent_proposal(paper_id, &request, &manifest.manifest_hash, &binding)?;
        let authority_now = Utc::now();
        if lease.expires_at <= authority_now {
            return Err(ApiError::conflict(
                "agent_proposal_requires_current_lease",
                "Agent proposal binding must hold the current unexpired section lease and fencing token",
            ));
        }
        let proposal = AgentProposal {
            proposal_id: request.proposal_id,
            paper_project_id: paper_id,
            work_item_id: request.work_item_id,
            section_key: request.section_key.clone(),
            parent_revision_id: request.parent_revision_id,
            lease_id: request.lease_id,
            lease_fencing_token: request.lease_fencing_token,
            expected_work_version: request.expected_work_version,
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
            updated_at: authority_now,
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
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("paper-section:{paper_id}:{}", request.section_key))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
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
    let team_row = sqlx::query(
        "select team_id,record_json from hepta_research_teams where team_id=$1 for share",
    )
    .bind(paper.team_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    if team.team_id != team_row.get::<Uuid, _>("team_id") || team.team_id != paper.team_id {
        return Err(ApiError::internal(
            "research team relational identity disagrees with record_json",
        ));
    }
    let scope_row = sqlx::query(
        "select w.record_json as work_json,
                w.paper_project_id as work_paper_id,
                w.assigned_player_id as work_player_id,
                w.assigned_binding_id as work_binding_id,
                w.status as work_status,w.version as work_version,
                m.record_json as manifest_json,
                m.manifest_id as relational_manifest_id,
                m.paper_project_id as manifest_paper_id,
                m.manifest_hash as relational_manifest_hash,
                m.version as manifest_version,
                b.record_json as binding_json,
                b.binding_id as relational_binding_id,
                b.player_id as binding_player_id,
                b.agent_id as relational_agent_id,
                b.status as binding_status,b.version as binding_version,
                tm.player_id as roster_player_id,tm.binding_id as roster_binding_id
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
    let binding: AgentBinding = decode_record(scope_row.get("binding_json"), "Agent binding")?;
    let work: WorkItem = decode_record(scope_row.get("work_json"), "work item")?;
    let work_version = u64::try_from(scope_row.get::<i64, _>("work_version"))
        .map_err(|_| ApiError::internal("negative work item version"))?;
    let manifest_version = u64::try_from(scope_row.get::<i64, _>("manifest_version"))
        .map_err(|_| ApiError::internal("negative artifact manifest version"))?;
    let binding_version = u64::try_from(scope_row.get::<i64, _>("binding_version"))
        .map_err(|_| ApiError::internal("negative Agent binding version"))?;
    if work.work_item_id != request.work_item_id
        || work.paper_project_id != scope_row.get::<Uuid, _>("work_paper_id")
        || work.assigned_player_id != scope_row.get::<Option<Uuid>, _>("work_player_id")
        || work.assigned_binding_id != scope_row.get::<Option<Uuid>, _>("work_binding_id")
        || work.status.as_str() != scope_row.get::<String, _>("work_status")
        || work.version != work_version
        || manifest.manifest_id != request.artifact_manifest_id
        || manifest.manifest_id != scope_row.get::<Uuid, _>("relational_manifest_id")
        || manifest.paper_project_id != scope_row.get::<Uuid, _>("manifest_paper_id")
        || manifest.manifest_hash != scope_row.get::<String, _>("relational_manifest_hash")
        || manifest.version != manifest_version
        || binding.binding_id != request.binding_id
        || binding.binding_id != scope_row.get::<Uuid, _>("relational_binding_id")
        || binding.agent_id != request.agent_id
        || binding.player_id != scope_row.get::<Uuid, _>("binding_player_id")
        || binding.agent_id != scope_row.get::<String, _>("relational_agent_id")
        || binding.status.as_str() != scope_row.get::<String, _>("binding_status")
        || binding.version != binding_version
        || binding.player_id != scope_row.get::<Uuid, _>("roster_player_id")
        || binding.binding_id != scope_row.get::<Uuid, _>("roster_binding_id")
    {
        return Err(ApiError::internal(
            "Agent proposal relational scope disagrees with record_json",
        ));
    }
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
    if !matches!(
        work.status,
        WorkItemStatus::Planned | WorkItemStatus::InProgress | WorkItemStatus::Review
    ) {
        return Err(ApiError::conflict(
            "agent_proposal_work_not_active",
            "Agent proposals require work in planned, in_progress, or review status",
        ));
    }
    if work.version != request.expected_work_version {
        return Err(version_conflict(
            "Agent proposal work item",
            request.expected_work_version,
            work.version,
        ));
    }
    let head_row = sqlx::query(
        "select paper_project_id,section_key,current_head_revision_id,fencing_token,version
         from hepta_section_heads
         where paper_project_id=$1 and section_key=$2
         for share",
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
    let head_fencing_token = u64::try_from(head_row.get::<i64, _>("fencing_token"))
        .map_err(|_| ApiError::internal("negative section fencing token"))?;
    let head_version = u64::try_from(head_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("negative section head version"))?;
    if head_row.get::<Uuid, _>("paper_project_id") != paper_id
        || head_row.get::<String, _>("section_key") != request.section_key
        || head_fencing_token != request.lease_fencing_token
        || head_version == 0
    {
        return Err(ApiError::conflict(
            "stale_agent_proposal_lease_epoch",
            "Agent proposal lease fencing token is not the current section epoch",
        ));
    }
    let lease_rows = sqlx::query(
        "select lease_id,paper_project_id,section_key,holder_player_id,holder_binding_id,
                fencing_token,status,version,acquired_at,expires_at,updated_at,record_json
         from hepta_section_leases
         where paper_project_id=$1 and section_key=$2 and status='active'
         for share",
    )
    .bind(paper_id)
    .bind(&request.section_key)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let lease_row = match lease_rows.as_slice() {
        [lease_row] => lease_row,
        _ => {
            return Err(ApiError::conflict(
                "agent_proposal_requires_current_lease",
                "Agent proposals require exactly one current active section lease",
            ));
        }
    };
    let lease: SectionLease = decode_record(lease_row.get("record_json"), "section lease")?;
    let relational_fencing_token = u64::try_from(lease_row.get::<i64, _>("fencing_token"))
        .map_err(|_| ApiError::internal("negative section lease fencing token"))?;
    let relational_lease_version = u64::try_from(lease_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("negative section lease version"))?;
    let relational_acquired_at: DateTime<Utc> = lease_row.get("acquired_at");
    let relational_expires_at: DateTime<Utc> = lease_row.get("expires_at");
    let relational_updated_at: DateTime<Utc> = lease_row.get("updated_at");
    if lease.lease_id != lease_row.get::<Uuid, _>("lease_id")
        || lease.paper_project_id != lease_row.get::<Uuid, _>("paper_project_id")
        || lease.section_key != lease_row.get::<String, _>("section_key")
        || lease.holder_player_id != lease_row.get::<Uuid, _>("holder_player_id")
        || lease.holder_binding_id != lease_row.get::<Uuid, _>("holder_binding_id")
        || lease.fencing_token != relational_fencing_token
        || lease.status.as_str() != lease_row.get::<String, _>("status")
        || lease.version != relational_lease_version
        || lease.acquired_at.timestamp_micros() != relational_acquired_at.timestamp_micros()
        || lease.expires_at.timestamp_micros() != relational_expires_at.timestamp_micros()
        || lease.updated_at.timestamp_micros() != relational_updated_at.timestamp_micros()
    {
        return Err(ApiError::internal(
            "section lease relational columns disagree with record_json",
        ));
    }
    if lease.status != SectionLeaseStatus::Active
        || lease.paper_project_id != paper_id
        || lease.section_key != request.section_key
        || lease.lease_id != request.lease_id
        || lease.holder_player_id != binding.player_id
        || lease.holder_binding_id != request.binding_id
        || lease.fencing_token != request.lease_fencing_token
        || lease.fencing_token != head_fencing_token
    {
        return Err(ApiError::conflict(
            "agent_proposal_requires_current_lease",
            "Agent proposal binding must hold the current unexpired section lease and fencing token",
        ));
    }
    let public_key = verify_agent_proposal(paper_id, &request, &manifest.manifest_hash, &binding)?;
    let authority_now: DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    if lease.expires_at <= authority_now {
        return Err(ApiError::conflict(
            "agent_proposal_requires_current_lease",
            "Agent proposal binding must hold the current unexpired section lease and fencing token",
        ));
    }
    let proposal = AgentProposal {
        proposal_id: request.proposal_id,
        paper_project_id: paper_id,
        work_item_id: request.work_item_id,
        section_key: request.section_key.clone(),
        parent_revision_id: request.parent_revision_id,
        lease_id: request.lease_id,
        lease_fencing_token: request.lease_fencing_token,
        expected_work_version: request.expected_work_version,
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
        updated_at: authority_now,
    };
    sqlx::query(
        "insert into hepta_agent_proposals (
            proposal_id,paper_project_id,work_item_id,section_key,parent_revision_id,
            lease_id,lease_fencing_token,expected_work_version,
            proposal_kind,payload_hash,artifact_manifest_id,artifact_manifest_hash,agent_id,binding_id,
            agent_key_id,agent_public_key,signature,status,version,record_json,signed_at,updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,'submitted',1,$18::jsonb,$19,$20)",
    )
    .bind(proposal.proposal_id)
    .bind(paper_id)
    .bind(proposal.work_item_id)
    .bind(&proposal.section_key)
    .bind(proposal.parent_revision_id)
    .bind(proposal.lease_id)
    .bind(
        i64::try_from(proposal.lease_fencing_token)
            .map_err(|_| ApiError::internal("lease fencing token exceeds PostgreSQL bigint"))?,
    )
    .bind(
        i64::try_from(proposal.expected_work_version)
            .map_err(|_| ApiError::internal("work version exceeds PostgreSQL bigint"))?,
    )
    .bind(proposal.proposal_kind.as_str())
    .bind(&proposal.payload_hash)
    .bind(proposal.artifact_manifest_id)
    .bind(&proposal.artifact_manifest_hash)
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
        if request.decision == HumanDecisionKind::Accept {
            let artifact_manifest_id = memory
                .collaboration
                .proposals
                .get(&request.proposal_id)
                .filter(|proposal| proposal.paper_project_id == paper_id)
                .map(|proposal| proposal.artifact_manifest_id)
                .ok_or_else(|| {
                    ApiError::bad_request(
                        "cross_paper_or_missing_proposal",
                        "proposal is missing or belongs to another paper",
                    )
                })?;
            if memory.collaboration.proposals.values().any(|proposal| {
                proposal.proposal_id != request.proposal_id
                    && proposal.artifact_manifest_id == artifact_manifest_id
                    && proposal.status == AgentProposalStatus::Accepted
            }) {
                return Err(ApiError::conflict(
                    "artifact_contribution_duplicated",
                    "one artifact manifest may have only one accepted proposal globally",
                ));
            }
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
    if request.decision == HumanDecisionKind::Accept {
        let already_accepted: bool = sqlx::query_scalar(
            "select exists(
                select 1 from hepta_agent_proposals
                where artifact_manifest_id=$1 and status='accepted' and proposal_id<>$2
             )",
        )
        .bind(proposal.artifact_manifest_id)
        .bind(proposal.proposal_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        if already_accepted {
            return Err(ApiError::conflict(
                "artifact_contribution_duplicated",
                "one artifact manifest may have only one accepted proposal globally",
            ));
        }
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
    .map_err(|error| match &error {
        sqlx::Error::Database(database)
            if database.is_unique_violation() && request.decision == HumanDecisionKind::Accept =>
        {
            ApiError::conflict(
                "artifact_contribution_duplicated",
                "one artifact manifest may have only one accepted proposal globally",
            )
        }
        _ => ApiError::database(error),
    })?;
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
        require_collaboration_phase(&paper, CollaborationMutation::SectionMerge)?;
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
    require_collaboration_phase(&paper, CollaborationMutation::SectionMerge)?;
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
    ensure_automatic_challenge_expiry_materialized(&state, paper_id, &assertion, Utc::now())
        .await?;
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
        validate_paper_role_resources(&paper)?;
        let author_raid_progress = project_author_raid_progress(
            &paper,
            &team,
            assertion.player_id,
            &work_items,
            &paper_revisions,
            &revision_artifact_bindings,
            &authorship_consents,
            &joint_submission,
            &artifact_manifests,
            &evidence_cards,
            &citations,
            &experiment_plans,
            &runs,
            &claims,
            &section_heads,
            &section_revisions,
            &section_reviews,
            &section_merges,
        );
        return Ok(Json(PaperRoomReadModel {
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
    validate_paper_role_resources(&paper)?;
    let author_raid_progress = project_author_raid_progress(
        &paper,
        &team,
        assertion.player_id,
        &work_items,
        &paper_revisions,
        &revision_artifact_bindings,
        &authorship_consents,
        &joint_submission,
        &artifact_manifests,
        &evidence_cards,
        &citations,
        &experiment_plans,
        &runs,
        &claims,
        &section_heads,
        &section_revisions,
        &section_reviews,
        &section_merges,
    );
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(PaperRoomReadModel {
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
    ensure_automatic_challenge_expiry_materialized(&state, paper_id, &assertion, Utc::now())
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
