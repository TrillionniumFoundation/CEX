use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    paper_raid_contracts::{
        agent_capability_disclosure_hash, authorship_consent_signing_bytes, canonical_json_bytes,
        canonical_json_sha256, consumer_user_assertion_signing_bytes, paper_bundle_hash,
        paper_release_candidate_hash, research_session_roster_root, section_materialization_root,
        sha256_digest, sign_authorization_set_consumption_receipt, sign_nakama_completion_receipt,
        sign_research_session_authorization, verify_agent_binding_key_rotation_signatures,
        verify_agent_binding_proof, verify_agent_binding_proof_v3,
        verify_human_key_registration_pop, verify_human_key_revocation_signature,
        verify_human_key_rotation_signatures, verify_team_member_acceptance_signature,
        AgentBindingKeyRotationClaimV2, AgentBindingProofClaimV2, AgentBindingProofClaimV3,
        AgentCapabilityDisclosureV1, AuthorshipConsentSigningV2, ConsumerUserAssertionClaimV2,
        HumanKeyRegistrationClaimV2, HumanKeyRevocationClaimV2, HumanKeyRotationClaimV2,
        PaperBundleAuthorConsentV2, PaperBundleV2, PaperReleaseAuthorV2, PaperReleaseCandidateV2,
        ResearchSessionAuthorizationClaimV1, ResearchSessionCompletionV1, ResearchSessionEventV1,
        ResearchSessionRosterMemberV1, SectionMaterializationDescriptorV1,
        SignedAuthorizationSetConsumptionReceiptV1, SignedConsumerUserAssertionV2,
        SignedNakamaCompletionReceiptV1, SignedResearchSessionAuthorizationV1,
        TeamMemberAcceptanceSigningV2, AGENT_BINDING_KEY_ROTATION_V2, AGENT_BINDING_PROOF_V2,
        AGENT_BINDING_PROOF_V3, AUTHORSHIP_CONSENT_V2, HUMAN_KEY_REGISTRATION_V2,
        HUMAN_KEY_REVOCATION_V2, HUMAN_KEY_ROTATION_V2, JSON_SAFE_U64_MAX, PAPER_BUNDLE_V2,
        PAPER_RAID_PROTOCOL_V2, PAPER_RELEASE_CANDIDATE_V2, PAPER_REWORK_V1,
        TEAM_MEMBER_ACCEPTANCE_V2,
    },
    require_service_token, validate_contract_text_api, validate_non_empty, ApiError, AppState,
    ChallengeForwardTransitionV1, ChallengeMinimumV1, ChallengeRequirementKindV1,
    ChallengeRulesetEnforcementV1, EventEnvelope, PaperChallengeRulesetSnapshotV1,
    ResearchChallenge, CHALLENGE_RULESET_SNAPSHOT_V1, NAKAMA_TOKEN_HEADER, USER_ASSERTION_HEADER,
};

#[path = "paper_collaboration_v3.rs"]
mod collaboration_v3;
pub use collaboration_v3::*;

#[path = "paper_review_v4.rs"]
mod review_v4;
pub use review_v4::*;

#[path = "paper_rework_v1.rs"]
mod rework_v1;
pub use rework_v1::*;

#[path = "paper_raid_v2/nakama_control_v2.rs"]
mod nakama_control_v2;
pub use nakama_control_v2::*;

const PAPER_RAID_EVENT_SCHEMA_V2: &str = "hepta.paper_raid.event.v2";

#[derive(Clone, Default)]
pub(crate) struct PaperRaidMemory {
    players: HashMap<Uuid, HumanPlayer>,
    players_by_subject: HashMap<String, Uuid>,
    players_by_nakama_user: HashMap<Uuid, Uuid>,
    human_signing_keys: HashMap<(Uuid, String), HumanSigningKey>,
    bindings: HashMap<Uuid, AgentBinding>,
    active_bindings_by_agent: HashMap<String, Uuid>,
    used_agent_binding_nonces: HashSet<(String, String)>,
    used_agent_binding_rotation_nonces: HashSet<(Uuid, String)>,
    used_agent_binding_rotation_ids: HashSet<Uuid>,
    teams: HashMap<Uuid, ResearchTeam>,
    team_acceptances: HashMap<Uuid, TeamMemberAcceptance>,
    pub(crate) papers: HashMap<Uuid, PaperProject>,
    work_items: HashMap<Uuid, WorkItem>,
    revisions: HashMap<Uuid, PaperRevision>,
    consents: HashMap<Uuid, AuthorshipConsent>,
    pub(crate) submissions: HashMap<Uuid, JointPaperSubmission>,
    pub(crate) reworks: HashMap<Uuid, PaperReworkRecordV1>,
    pub(crate) rework_resubmissions: HashMap<Uuid, PaperReworkResubmissionV1>,
    pub(crate) research_session_authorization_sets:
        HashMap<(String, u64), ResearchSessionAuthorizationSetV1>,
    research_session_consumption_receipts:
        HashMap<(String, u64), SignedAuthorizationSetConsumptionReceiptV1>,
    pub(crate) research_session_completions:
        HashMap<(String, u64), SignedNakamaCompletionReceiptV1>,
    contribution_ledger_reservations: HashMap<Uuid, ContributionLedgerReservation>,
    nakama_control_commands: HashMap<Uuid, nakama_control_v2::StoredControlCommand>,
    nakama_control_idempotency: HashMap<(String, String), Uuid>,
    collaboration: collaboration_v3::CollaborationMemory,
    pub(crate) review: review_v4::ReviewMemory,
    idempotency: HashMap<String, MemoryIdempotencyRecord>,
    events: Vec<EventEnvelope>,
}

#[derive(Clone)]
struct ContributionLedgerReservation {
    paper_project_id: Uuid,
    release_candidate_hash: String,
}

#[cfg(test)]
impl PaperRaidMemory {
    pub(crate) fn paper_finality_v1_side_effect_snapshot(&self) -> Value {
        let mut authorization_sets = self
            .research_session_authorization_sets
            .values()
            .collect::<Vec<_>>();
        authorization_sets.sort_by(|left, right| {
            (&left.session_id, left.roster_version).cmp(&(&right.session_id, right.roster_version))
        });
        let mut completions = self
            .research_session_completions
            .values()
            .collect::<Vec<_>>();
        completions.sort_by(|left, right| {
            (&left.session_id, left.roster_version).cmp(&(&right.session_id, right.roster_version))
        });
        let mut idempotency = self
            .idempotency
            .iter()
            .map(|(key, record)| {
                json!({
                    "key": key,
                    "request_hash": record.request_hash,
                    "status": record.status.as_u16(),
                    "response": record.response,
                })
            })
            .collect::<Vec<_>>();
        idempotency.sort_by(|left, right| left["key"].as_str().cmp(&right["key"].as_str()));
        json!({
            "papers": self.papers,
            "submissions": self.submissions,
            "reworks": self.reworks,
            "rework_resubmissions": self.rework_resubmissions,
            "authorization_sets": authorization_sets,
            "completions": completions,
            "evaluations": self.review.evaluations,
            "reproductions": self.review.reproductions,
            "appeals": self.review.appeals,
            "resolutions": self.review.resolutions,
            "idempotency": idempotency,
            "events": self.events,
        })
    }
}

#[derive(Debug, Clone)]
struct MemoryIdempotencyRecord {
    request_hash: String,
    status: StatusCode,
    response: Value,
}

pub(crate) async fn operational_metrics(state: &AppState) -> Result<String, ApiError> {
    let (
        storage_backend,
        papers_total,
        active_authorization_epochs,
        pending_control_commands,
        oldest_pending_control_seconds,
        max_pending_control_attempts,
        pending_outbox_events,
        oldest_pending_outbox_seconds,
        max_pending_outbox_attempts,
        idempotency_records,
    ) = if let Some(pool) = &state.pool {
        let row = sqlx::query(
            "select \
                (select count(*) from hepta_paper_projects)::bigint as papers_total, \
                (select count(*) from hepta_research_session_authorization_sets \
                    where status in ('issued', 'consumed'))::bigint as active_authorization_epochs, \
                (select count(*) from hepta_nakama_research_control_commands \
                    where status = 'pending')::bigint as pending_control_commands, \
                greatest(coalesce((select extract(epoch from now() - min(created_at))::bigint \
                    from hepta_nakama_research_control_commands \
                    where status = 'pending'), 0), 0)::bigint as oldest_pending_control_seconds, \
                coalesce((select max(attempt_count) \
                    from hepta_nakama_research_control_commands \
                    where status = 'pending'), 0)::bigint as max_pending_control_attempts, \
                (select count(*) from hepta_outbox \
                    where delivered_at is null)::bigint as pending_outbox_events, \
                greatest(coalesce((select extract(epoch from now() - min(occurred_at))::bigint \
                    from hepta_outbox where delivered_at is null), 0), 0)::bigint \
                    as oldest_pending_outbox_seconds, \
                coalesce((select max(attempt_count) from hepta_outbox \
                    where delivered_at is null), 0)::bigint as max_pending_outbox_attempts, \
                (select count(*) from hepta_paper_raid_idempotency)::bigint as idempotency_records",
        )
        .fetch_one(pool)
        .await
        .map_err(ApiError::database)?;
        (
            "postgres",
            row.get::<i64, _>("papers_total"),
            row.get::<i64, _>("active_authorization_epochs"),
            row.get::<i64, _>("pending_control_commands"),
            row.get::<i64, _>("oldest_pending_control_seconds"),
            row.get::<i64, _>("max_pending_control_attempts"),
            row.get::<i64, _>("pending_outbox_events"),
            row.get::<i64, _>("oldest_pending_outbox_seconds"),
            row.get::<i64, _>("max_pending_outbox_attempts"),
            row.get::<i64, _>("idempotency_records"),
        )
    } else {
        let memory = state.paper_raid.read().await;
        (
            "memory",
            memory.papers.len() as i64,
            memory
                .research_session_authorization_sets
                .values()
                .filter(|authorization_set| {
                    matches!(
                        authorization_set.status,
                        ResearchSessionAuthorizationSetStatus::Issued
                            | ResearchSessionAuthorizationSetStatus::Consumed
                    )
                })
                .count() as i64,
            {
                let pending = memory
                    .nakama_control_commands
                    .values()
                    .filter(|command| {
                        command.record.status
                            == nakama_control_v2::NakamaResearchControlCommandStatusV2::Pending
                    })
                    .collect::<Vec<_>>();
                pending.len() as i64
            },
            memory
                .nakama_control_commands
                .values()
                .filter(|command| {
                    command.record.status
                        == nakama_control_v2::NakamaResearchControlCommandStatusV2::Pending
                })
                .map(|command| {
                    Utc::now()
                        .signed_duration_since(command.record.created_at)
                        .num_seconds()
                        .max(0)
                })
                .max()
                .unwrap_or(0),
            memory
                .nakama_control_commands
                .values()
                .filter(|command| {
                    command.record.status
                        == nakama_control_v2::NakamaResearchControlCommandStatusV2::Pending
                })
                .filter_map(|command| i64::try_from(command.record.attempt_count).ok())
                .max()
                .unwrap_or(0),
            0,
            0,
            0,
            memory.idempotency.len() as i64,
        )
    };

    Ok(format!(
        "# HELP hepta_paper_raid_storage_backend_info Active Paper Raid storage backend.\n\
         # TYPE hepta_paper_raid_storage_backend_info gauge\n\
         hepta_paper_raid_storage_backend_info{{backend=\"{storage_backend}\"}} 1\n\
         # HELP hepta_paper_raid_papers_total Number of Paper Raid projects.\n\
         # TYPE hepta_paper_raid_papers_total gauge\n\
         hepta_paper_raid_papers_total {papers_total}\n\
         # HELP hepta_paper_raid_active_authorization_epochs Active issued or consumed research-session authorization epochs.\n\
         # TYPE hepta_paper_raid_active_authorization_epochs gauge\n\
         hepta_paper_raid_active_authorization_epochs {active_authorization_epochs}\n\
         # HELP hepta_paper_raid_pending_control_commands Nakama research-control commands waiting to be applied.\n\
         # TYPE hepta_paper_raid_pending_control_commands gauge\n\
         hepta_paper_raid_pending_control_commands {pending_control_commands}\n\
         # HELP hepta_paper_raid_oldest_pending_control_seconds Age of the oldest pending Nakama research-control command.\n\
         # TYPE hepta_paper_raid_oldest_pending_control_seconds gauge\n\
         hepta_paper_raid_oldest_pending_control_seconds {oldest_pending_control_seconds}\n\
         # HELP hepta_paper_raid_max_pending_control_attempts Highest attempt count among pending Nakama research-control commands.\n\
         # TYPE hepta_paper_raid_max_pending_control_attempts gauge\n\
         hepta_paper_raid_max_pending_control_attempts {max_pending_control_attempts}\n\
         # HELP hepta_paper_raid_pending_outbox_events Transactional outbox events waiting for delivery.\n\
         # TYPE hepta_paper_raid_pending_outbox_events gauge\n\
         hepta_paper_raid_pending_outbox_events {pending_outbox_events}\n\
         # HELP hepta_paper_raid_oldest_pending_outbox_seconds Age of the oldest undelivered transactional outbox event.\n\
         # TYPE hepta_paper_raid_oldest_pending_outbox_seconds gauge\n\
         hepta_paper_raid_oldest_pending_outbox_seconds {oldest_pending_outbox_seconds}\n\
         # HELP hepta_paper_raid_max_pending_outbox_attempts Highest delivery attempt count among undelivered outbox events.\n\
         # TYPE hepta_paper_raid_max_pending_outbox_attempts gauge\n\
         hepta_paper_raid_max_pending_outbox_attempts {max_pending_outbox_attempts}\n\
         # HELP hepta_paper_raid_idempotency_records Durable Paper Raid idempotency records.\n\
         # TYPE hepta_paper_raid_idempotency_records gauge\n\
         hepta_paper_raid_idempotency_records {idempotency_records}\n"
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HumanPlayerStatus {
    Active,
    Suspended,
}

impl HumanPlayerStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanPlayer {
    pub player_id: Uuid,
    pub subject_id: String,
    pub nakama_user_id: Uuid,
    pub display_name: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub status: HumanPlayerStatus,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHumanPlayerRequest {
    pub player_id: Uuid,
    pub display_name: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub key_issued_at_unix: i64,
    pub key_expires_at_unix: i64,
    pub key_proof_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HumanSigningKeyStatus {
    Active,
    Rotated,
    Revoked,
}

impl HumanSigningKeyStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Rotated => "rotated",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HumanSigningKey {
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub status: HumanSigningKeyStatus,
    pub version: u64,
    pub registered_at: DateTime<Utc>,
    pub retired_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotateHumanSigningKeyRequest {
    pub rotation_id: Uuid,
    pub expected_player_version: u64,
    pub new_signing_key_id: String,
    pub new_signing_public_key: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub old_key_signature: String,
    pub new_key_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeHumanSigningKeyRequest {
    pub revocation_id: Uuid,
    pub expected_player_version: u64,
    pub reason_hash: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentBindingStatus {
    Active,
    Revoked,
}

impl AgentBindingStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBinding {
    pub binding_id: Uuid,
    pub player_id: Uuid,
    pub agent_id: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    pub agent_public_key_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_disclosure: Option<AgentCapabilityDisclosureV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_disclosure_hash: Option<String>,
    pub status: AgentBindingStatus,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentBindingRequest {
    pub binding_id: Uuid,
    pub player_id: Uuid,
    pub agent_id: String,
    pub agent_key_id: String,
    pub agent_public_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_proof_schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_disclosure: Option<AgentCapabilityDisclosureV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_disclosure_hash: Option<String>,
    pub agent_proof_nonce: String,
    pub agent_proof_issued_at_unix: i64,
    pub agent_proof_expires_at_unix: i64,
    pub agent_proof_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotateAgentBindingKeyRequest {
    pub rotation_id: Uuid,
    pub expected_binding_version: u64,
    pub agent_id: String,
    pub old_agent_key_id: String,
    pub old_agent_public_key: String,
    pub new_agent_key_id: String,
    pub new_agent_public_key: String,
    pub issued_at_unix: i64,
    pub expires_at_unix: i64,
    pub old_key_signature: String,
    pub new_key_signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamStatus {
    Forming,
    Locked,
    Archived,
}

impl TeamStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Forming => "forming",
            Self::Locked => "locked",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamMember {
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchTeam {
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub collaboration_compact_hash: String,
    pub status: TeamStatus,
    pub roster_version: u64,
    pub members: Vec<TeamMember>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTeamMemberRequest {
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateResearchTeamRequest {
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub collaboration_compact_hash: String,
    pub members: Vec<CreateTeamMemberRequest>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockResearchTeamRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamMemberAcceptance {
    pub acceptance_id: Uuid,
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub roster_version: u64,
    pub participant_slot: u32,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub agent_id: String,
    pub role: String,
    pub collaboration_compact_hash: String,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub accepted_at: DateTime<Utc>,
    pub superseded_at: Option<DateTime<Utc>>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptResearchTeamMembershipRequest {
    pub acceptance_id: Uuid,
    pub expected_team_version: u64,
    pub roster_version: u64,
    pub participant_slot: u32,
    pub binding_id: Uuid,
    pub role: String,
    pub collaboration_compact_hash: String,
    pub accepted_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperPhase {
    Forming,
    Preregistering,
    Researching,
    Experimenting,
    Drafting,
    IntegrityReview,
    Reproducing,
    AuthorApproval,
    IntegrityHold,
    SubmissionReady,
}

impl PaperPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Forming => "forming",
            Self::Preregistering => "preregistering",
            Self::Researching => "researching",
            Self::Experimenting => "experimenting",
            Self::Drafting => "drafting",
            Self::IntegrityReview => "integrity_review",
            Self::Reproducing => "reproducing",
            Self::AuthorApproval => "author_approval",
            Self::IntegrityHold => "integrity_hold",
            Self::SubmissionReady => "submission_ready",
        }
    }

    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Forming, Self::Preregistering)
                | (Self::Preregistering, Self::Researching)
                | (Self::Researching, Self::Experimenting)
                | (Self::Experimenting, Self::Drafting)
                | (Self::Drafting, Self::IntegrityReview)
                | (Self::IntegrityReview, Self::Reproducing)
                | (Self::IntegrityReview, Self::Drafting)
                | (Self::Reproducing, Self::AuthorApproval)
                | (Self::Reproducing, Self::Drafting)
                | (Self::AuthorApproval, Self::Drafting)
                | (Self::IntegrityHold, Self::Drafting)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperProject {
    pub paper_project_id: Uuid,
    pub team_id: Uuid,
    pub challenge_id: Uuid,
    pub title: String,
    pub target_format: String,
    pub phase: PaperPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_ruleset_snapshot: Option<PaperChallengeRulesetSnapshotV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub challenge_ruleset_snapshot_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_rework_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_rework_cycle: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rework_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub outcome: PaperChallengeOutcomeV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_resources: Option<RoleResourceStateV1>,
    pub current_revision_id: Option<Uuid>,
    pub release_candidate_revision_id: Option<Uuid>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperChallengeOutcomeV1 {
    #[default]
    InProgress,
    SubmissionReady,
    Failed,
    Expired,
    Abandoned,
}

impl PaperChallengeOutcomeV1 {
    fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::SubmissionReady => "submission_ready",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperProjectRequest {
    pub paper_project_id: Uuid,
    pub team_id: Uuid,
    pub title: String,
    pub target_format: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionPaperRequest {
    pub expected_version: u64,
    pub next_phase: PaperPhase,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionPaperOutcomeRequestV1 {
    pub expected_version: u64,
    pub outcome: PaperChallengeOutcomeV1,
    pub reason_code: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemStatus {
    Planned,
    InProgress,
    Review,
    Accepted,
    Rejected,
    Cancelled,
}

impl WorkItemStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItem {
    pub work_item_id: Uuid,
    pub paper_project_id: Uuid,
    pub kind: String,
    pub title: String,
    pub assigned_player_id: Option<Uuid>,
    pub assigned_binding_id: Option<Uuid>,
    pub status: WorkItemStatus,
    pub artifact_manifest_hash: Option<String>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkItemRequest {
    pub work_item_id: Uuid,
    pub expected_paper_version: u64,
    pub kind: String,
    pub title: String,
    pub assigned_player_id: Option<Uuid>,
    pub assigned_binding_id: Option<Uuid>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionWorkItemRequest {
    pub expected_version: u64,
    pub next_status: WorkItemStatus,
    pub artifact_manifest_hash: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaperRevisionStatus {
    Draft,
    ReleaseCandidate,
    Superseded,
}

impl PaperRevisionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::ReleaseCandidate => "release_candidate",
            Self::Superseded => "superseded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperRevision {
    pub revision_id: Uuid,
    pub paper_project_id: Uuid,
    pub parent_revision_id: Option<Uuid>,
    pub revision_number: u64,
    pub source_manifest_hash: String,
    pub artifact_manifest_hash: String,
    pub bibliography_hash: String,
    pub claim_evidence_graph_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_materialization: Option<SectionMaterializationDescriptorV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_materialization_root: Option<String>,
    pub status: PaperRevisionStatus,
    pub release_candidate: Option<PaperReleaseCandidateV2>,
    pub release_candidate_hash: Option<String>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePaperRevisionRequest {
    pub revision_id: Uuid,
    pub expected_paper_version: u64,
    pub parent_revision_id: Option<Uuid>,
    pub source_manifest_hash: String,
    pub artifact_manifest_hash: String,
    pub bibliography_hash: String,
    pub claim_evidence_graph_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromoteReleaseCandidateRequest {
    pub expected_paper_version: u64,
    pub expected_revision_version: u64,
    pub title: String,
    pub abstract_text: String,
    pub collaboration_compact_hash: String,
    pub research_protocol_snapshot_hash: String,
    pub ethics_disclosure_hash: String,
    pub coi_disclosure_hash: String,
    pub contribution_ledger_id: Uuid,
    pub contribution_ledger_hash: String,
    pub ai_disclosure_hash: String,
    pub license: String,
    pub authors: Vec<PaperReleaseAuthorV2>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromoteReleaseCandidateResponse {
    pub revision: PaperRevision,
    pub release_candidate_hash: String,
    pub paper_version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorshipConsent {
    pub consent_id: Uuid,
    pub paper_project_id: Uuid,
    pub revision_id: Uuid,
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub release_candidate_hash: String,
    pub signed_at: DateTime<Utc>,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAuthorshipConsentRequest {
    pub consent_id: Uuid,
    pub expected_paper_version: u64,
    pub revision_id: Uuid,
    pub player_id: Uuid,
    pub signing_key_id: String,
    pub signing_public_key: String,
    pub signing_public_key_hash: String,
    pub release_candidate_hash: String,
    pub signed_at_unix: i64,
    pub signature: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JointSubmissionStatus {
    SubmissionReady,
    IntegrityHold,
    Withdrawn,
}

impl JointSubmissionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::SubmissionReady => "submission_ready",
            Self::IntegrityHold => "integrity_hold",
            Self::Withdrawn => "withdrawn",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JointPaperSubmission {
    pub submission_id: Uuid,
    pub paper_project_id: Uuid,
    pub revision_id: Uuid,
    pub release_candidate_hash: String,
    pub paper_bundle_hash: String,
    pub status: JointSubmissionStatus,
    pub paper_bundle: PaperBundleV2,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalizePaperRequest {
    pub submission_id: Uuid,
    pub expected_paper_version: u64,
    pub revision_id: Uuid,
    pub release_candidate_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSessionAuthorizationSetStatus {
    Issued,
    Consumed,
    Completed,
    Superseded,
    Expired,
}

impl ResearchSessionAuthorizationSetStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Issued => "issued",
            Self::Consumed => "consumed",
            Self::Completed => "completed",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSessionAuthorizationMemberV1 {
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub authorization: SignedResearchSessionAuthorizationV1,
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchSessionAuthorizationSetV1 {
    pub schema: String,
    pub authorization_set_id: Uuid,
    pub session_id: String,
    pub team_id: Uuid,
    pub paper_project_id: Uuid,
    pub challenge_id: Uuid,
    /// Immutable author/team roster snapshot version. This never substitutes
    /// for the Nakama authorization epoch below.
    pub team_roster_version: u64,
    /// Logical-session authorization epoch. Initial issuance is 1 and every
    /// replacement increments it while retaining `session_id`.
    pub roster_version: u64,
    pub roster_root: String,
    pub supersedes_roster_version: Option<u64>,
    pub replaced_participant_slot: Option<u32>,
    pub status: ResearchSessionAuthorizationSetStatus,
    pub members: Vec<ResearchSessionAuthorizationMemberV1>,
    pub version: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueResearchSessionAuthorizationSetRequest {
    pub session_id: String,
    pub paper_project_id: Uuid,
    pub expected_paper_version: u64,
    pub expected_team_version: u64,
    pub ttl_seconds: Option<u64>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceResearchSessionAuthorizationSetRequest {
    pub paper_project_id: Uuid,
    pub expected_paper_version: u64,
    pub expected_team_version: u64,
    pub previous_roster_version: u64,
    pub disconnected_participant_slot: u32,
    pub ttl_seconds: Option<u64>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumeResearchSessionAuthorizationSetRequest {
    pub schema: String,
    pub session_id: String,
    pub roster_version: u64,
    pub roster_root: String,
    pub authorization_ids: Vec<Uuid>,
    pub consumed_at_unix: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestNakamaResearchSessionCompletionRequestV1 {
    pub schema: String,
    pub completion: ResearchSessionCompletionV1,
    pub archive: Vec<ResearchSessionEventV1>,
    pub idempotency_key: String,
}

pub type NakamaResearchSessionCompletionReceiptV1 = SignedNakamaCompletionReceiptV1;
pub type AuthorizationSetConsumptionReceiptV1 = SignedAuthorizationSetConsumptionReceiptV1;

#[derive(Debug, Serialize)]
struct PaperRaidManifestResponse {
    service: &'static str,
    protocol_version: &'static str,
    human_authority: &'static str,
    agent_execution_mode: &'static str,
    supported_team_sizes: [u8; 3],
    persistence_model: &'static str,
    final_product: &'static str,
    real_time_authority: &'static str,
    team_formation_authority: &'static str,
    nakama_completion_trust: &'static str,
    finality_mode: &'static str,
    collaboration_protocol: &'static str,
    artifact_binding_schema: &'static str,
    source_artifact_bundle_schema: &'static str,
    source_artifact_bundle_contract_hash: &'static str,
    artifact_adapter_source_revision: &'static str,
    review_protocol: &'static str,
    rework_protocol: &'static str,
    rework_lease_seconds: u64,
    rework_content_commitment_schema: &'static str,
    challenge_ruleset_protocol: &'static str,
    legacy_challenge_policy: &'static str,
    tolerance_policy_schema: &'static str,
    settlement_authority: &'static str,
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/v2/hepta/manifest", get(manifest))
        .route("/v2/hepta/openapi.yaml", get(openapi))
        .route("/v2/hepta/players", post(create_player))
        .route("/v2/hepta/players/me", get(get_self_player))
        .route(
            "/v2/hepta/players/:player_id/signing-key/rotate",
            post(rotate_human_signing_key),
        )
        .route(
            "/v2/hepta/players/:player_id/signing-key/revoke",
            post(revoke_human_signing_key),
        )
        .route(
            "/v2/hepta/agent-bindings",
            get(list_self_agent_bindings).post(create_agent_binding),
        )
        .route(
            "/v2/hepta/agent-bindings/:binding_id/rotate-key",
            post(rotate_agent_binding_key),
        )
        .route("/v2/hepta/teams", post(create_team))
        .route("/v2/hepta/teams/:team_id", get(get_team))
        .route(
            "/v2/hepta/teams/:team_id/member-acceptances",
            get(list_team_acceptances).post(accept_team_membership),
        )
        .route("/v2/hepta/teams/:team_id/lock", post(lock_team))
        .route("/v2/hepta/papers", post(create_paper))
        .route("/v2/hepta/papers/:paper_id", get(get_paper))
        .route(
            "/v2/hepta/papers/:paper_id/outcome",
            post(transition_paper_outcome),
        )
        .route(
            "/v2/hepta/papers/:paper_id/transition",
            post(transition_paper),
        )
        .route(
            "/v2/hepta/papers/:paper_id/work-items",
            post(create_work_item),
        )
        .route(
            "/v2/hepta/work-items/:work_item_id/transition",
            post(transition_work_item),
        )
        .route(
            "/v2/hepta/papers/:paper_id/revisions",
            post(create_revision),
        )
        .route(
            "/v2/hepta/papers/:paper_id/revisions/:revision_id/promote",
            post(promote_release_candidate),
        )
        .route(
            "/v2/hepta/papers/:paper_id/author-consents",
            post(create_authorship_consent),
        )
        .route("/v2/hepta/papers/:paper_id/finalize", post(finalize_paper))
        .route(
            "/v2/hepta/papers/:paper_id/submission",
            get(get_joint_submission),
        )
        .route(
            "/v2/hepta/research-session-authorizations",
            post(issue_research_session_authorization_set),
        )
        .route(
            "/v2/hepta/research-session-authorizations/:session_id/replace",
            post(replace_research_session_authorization_set),
        )
        .route(
            "/v2/hepta/nakama/research-session-authorizations/consumed",
            post(consume_research_session_authorization_set),
        )
        .route(
            "/v2/hepta/nakama/research-session-completions",
            post(ingest_nakama_research_session_completion),
        )
        .merge(nakama_control_v2::router())
        .merge(collaboration_v3::router())
        .merge(review_v4::router())
        .merge(rework_v1::router())
}

async fn manifest(State(state): State<AppState>) -> Json<PaperRaidManifestResponse> {
    Json(PaperRaidManifestResponse {
        service: "hepta-research-league",
        protocol_version: PAPER_RAID_PROTOCOL_V2,
        human_authority: "human_players_sign_authorship_and_publication_release",
        agent_execution_mode: "external_only",
        supported_team_sizes: [3, 4, 5],
        persistence_model: "versioned_postgresql_aggregates_with_transactional_outbox",
        final_product: PAPER_BUNDLE_V2,
        real_time_authority: "nakama_research_session_v1",
        team_formation_authority:
            "all_human_members_sign_exact_roster_role_agent_binding_and_compact",
        nakama_completion_trust: "locally_pinned_authority_key_and_full_archive_verification",
        finality_mode: state.security.finality_mode.as_str(),
        collaboration_protocol: PAPER_COLLABORATION_PROTOCOL_V3,
        artifact_binding_schema: ARTIFACT_MANIFEST_BINDING_SCHEMA_V1,
        source_artifact_bundle_schema: SOURCE_ARTIFACT_BUNDLE_SCHEMA_V1,
        source_artifact_bundle_contract_hash: ARTIFACT_BUNDLE_ADAPTER_CONTRACT_HASH_V1,
        artifact_adapter_source_revision: ARTIFACT_BUNDLE_ADAPTER_SOURCE_REVISION,
        review_protocol: PAPER_REVIEW_PROTOCOL_V4,
        rework_protocol: PAPER_REWORK_V1,
        rework_lease_seconds: u64::try_from(rework_v1::PAPER_REWORK_LEASE_HOURS * 3_600)
            .expect("positive fixed rework lease"),
        rework_content_commitment_schema: "hepta.paper_raid.rework_content_commitment.v1",
        challenge_ruleset_protocol: crate::CHALLENGE_RULESET_V1,
        legacy_challenge_policy: "readable_conservative_gates_unranked_no_deadline",
        tolerance_policy_schema: TOLERANCE_POLICY_SCHEMA_V1,
        settlement_authority: "pending_finality_until_verified_chain_receipt",
    })
}

async fn openapi() -> &'static str {
    include_str!("../../../docs/openapi/hepta-paper-raid-v2.yaml")
}

fn validate_idempotency_key(value: &str) -> Result<(), ApiError> {
    validate_contract_text_api("idempotency_key", value)?;
    if value.len() > 200 {
        return Err(ApiError::bad_request(
            "invalid_idempotency_key",
            "idempotency_key exceeds 200 bytes",
        ));
    }
    Ok(())
}

fn validate_digest_v2(field: &'static str, value: &str) -> Result<(), ApiError> {
    crate::paper_raid_contracts::decode_digest(value)
        .map(|_| ())
        .map_err(|message| ApiError::bad_request("invalid_hash", format!("{field}: {message}")))
}

fn canonical_public_key(value: &str) -> Result<String, ApiError> {
    let key = crate::decode_verifying_key(value)?;
    let canonical = BASE64.encode(key.to_bytes());
    if canonical != value {
        return Err(ApiError::bad_request(
            "invalid_public_key",
            "signing_public_key must use canonical padded base64",
        ));
    }
    Ok(canonical)
}

fn request_hash<T: Serialize>(request: &T) -> Result<String, ApiError> {
    canonical_json_sha256(request)
        .map_err(|error| ApiError::internal(format!("serialize idempotent request: {error}")))
}

fn require_user_assertion(
    headers: &HeaderMap,
    state: &AppState,
    operation: &str,
    http_method: &str,
    canonical_path: &str,
    idempotency_key: &str,
    body_hash: &str,
) -> Result<ConsumerUserAssertionClaimV2, ApiError> {
    require_user_assertion_internal(
        headers,
        state,
        operation,
        http_method,
        canonical_path,
        idempotency_key,
        body_hash,
        true,
    )
}

fn require_user_assertion_for_applied_replay(
    headers: &HeaderMap,
    state: &AppState,
    operation: &str,
    http_method: &str,
    canonical_path: &str,
    idempotency_key: &str,
    body_hash: &str,
) -> Result<ConsumerUserAssertionClaimV2, ApiError> {
    require_user_assertion_internal(
        headers,
        state,
        operation,
        http_method,
        canonical_path,
        idempotency_key,
        body_hash,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn require_user_assertion_internal(
    headers: &HeaderMap,
    state: &AppState,
    operation: &str,
    http_method: &str,
    canonical_path: &str,
    idempotency_key: &str,
    body_hash: &str,
    enforce_current_time: bool,
) -> Result<ConsumerUserAssertionClaimV2, ApiError> {
    let encoded = headers
        .get(USER_ASSERTION_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::forbidden(
                "user_assertion_required",
                "a trusted Consumer Edge user assertion is required",
            )
        })?;
    let decoded = BASE64.decode(encoded).map_err(|_| {
        ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion must be canonical padded base64 JSON",
        )
    })?;
    if BASE64.encode(&decoded) != encoded {
        return Err(ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion must be canonical padded base64 JSON",
        ));
    }
    let assertion: SignedConsumerUserAssertionV2 =
        serde_json::from_slice(&decoded).map_err(|_| {
            ApiError::bad_request(
                "invalid_user_assertion",
                "user assertion JSON does not match the v2 contract",
            )
        })?;
    let canonical = canonical_json_bytes(&assertion)
        .map_err(|error| ApiError::internal(format!("canonicalize user assertion: {error}")))?;
    if canonical != decoded {
        return Err(ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion JSON must use canonical sorted-key encoding",
        ));
    }
    let claim = &assertion.claim;
    if claim.issuer != state.security.consumer_edge_issuer
        || claim.audience != state.security.consumer_edge_audience
        || claim.operation != operation
        || claim.http_method != http_method
        || claim.canonical_path != canonical_path
        || claim.idempotency_key != idempotency_key
        || claim.nonce != idempotency_key
        || claim.body_hash != body_hash
    {
        return Err(ApiError::forbidden(
            "user_assertion_scope_mismatch",
            "user assertion issuer, audience, request, or nonce scope does not match",
        ));
    }
    if enforce_current_time {
        enforce_user_assertion_time(claim)?;
    }
    let signing_bytes = consumer_user_assertion_signing_bytes(claim, &assertion.issuer_key_id)
        .map_err(|message| ApiError::bad_request("invalid_user_assertion", message))?;
    let signature = BASE64.decode(&assertion.signature).map_err(|_| {
        ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion signature must be canonical padded base64",
        )
    })?;
    if BASE64.encode(&signature) != assertion.signature {
        return Err(ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion signature must be canonical padded base64",
        ));
    }
    let signature = Signature::from_slice(&signature).map_err(|_| {
        ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion signature must decode to 64 bytes",
        )
    })?;
    state
        .security
        .consumer_edge_verifying_keys
        .get(&assertion.issuer_key_id)
        .ok_or_else(|| {
            ApiError::forbidden(
                "user_assertion_unknown_issuer_key",
                "Consumer Edge user assertion key ID is not trusted",
            )
        })?
        .verify(&signing_bytes, &signature)
        .map_err(|_| {
            ApiError::forbidden(
                "user_assertion_signature_failed",
                "Consumer Edge user assertion signature verification failed",
            )
        })?;
    Ok(claim.clone())
}

fn enforce_user_assertion_time(claim: &ConsumerUserAssertionClaimV2) -> Result<(), ApiError> {
    let now = Utc::now().timestamp();
    if claim.issued_at_unix > now + 60
        || claim.expires_at_unix < now - 30
        || claim.expires_at_unix - claim.issued_at_unix > 300
    {
        return Err(ApiError::forbidden(
            "user_assertion_expired",
            "user assertion is outside the allowed clock/TTL window",
        ));
    }
    Ok(())
}

fn enforce_onboarding_proof_time(
    issued_at_unix: i64,
    expires_at_unix: i64,
    code: &'static str,
    message: &'static str,
) -> Result<(), ApiError> {
    let now = Utc::now().timestamp();
    if now < issued_at_unix || now >= expires_at_unix || expires_at_unix - issued_at_unix > 600 {
        return Err(ApiError::forbidden(code, message));
    }
    Ok(())
}

fn require_member_read_assertion(
    headers: &HeaderMap,
    state: &AppState,
    operation: &str,
    canonical_path: &str,
) -> Result<ConsumerUserAssertionClaimV2, ApiError> {
    let encoded = headers
        .get(USER_ASSERTION_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ApiError::forbidden(
                "user_assertion_required",
                "a signed team-member read assertion is required",
            )
        })?;
    let decoded = BASE64.decode(encoded).map_err(|_| {
        ApiError::bad_request(
            "invalid_user_assertion",
            "user assertion must be canonical padded base64 JSON",
        )
    })?;
    let untrusted: SignedConsumerUserAssertionV2 =
        serde_json::from_slice(&decoded).map_err(|_| {
            ApiError::bad_request(
                "invalid_user_assertion",
                "user assertion JSON does not match the v2 contract",
            )
        })?;
    require_user_assertion(
        headers,
        state,
        operation,
        "GET",
        canonical_path,
        &untrusted.claim.idempotency_key,
        &sha256_digest(&[]),
    )
}

fn assert_player_identity(
    assertion: &ConsumerUserAssertionClaimV2,
    player: &HumanPlayer,
) -> Result<(), ApiError> {
    if assertion.player_id != player.player_id
        || assertion.subject_id != player.subject_id
        || assertion.nakama_user_id != player.nakama_user_id
    {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "Consumer user assertion does not match the stored human player identity",
        ));
    }
    Ok(())
}

fn assert_team_actor_memory(
    memory: &PaperRaidMemory,
    team: &ResearchTeam,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<(), ApiError> {
    if !team
        .members
        .iter()
        .any(|member| member.player_id == assertion.player_id)
    {
        return Err(ApiError::forbidden(
            "user_not_on_team",
            "asserted human player is not a member of the research team",
        ));
    }
    let player = memory
        .players
        .get(&assertion.player_id)
        .ok_or_else(|| ApiError::internal("asserted team member human player record is missing"))?;
    assert_player_identity(assertion, player)
}

const CANONICAL_AUTHOR_ROLES: [&str; 3] = ["captain", "evidence", "experiment"];
const SUPPORT_AUTHOR_ROLE: &str = "support";

fn canonical_author_roles_are_enforced(team: &ResearchTeam) -> bool {
    team.members.iter().all(|member| {
        member.role == SUPPORT_AUTHOR_ROLE || CANONICAL_AUTHOR_ROLES.contains(&member.role.as_str())
    }) && CANONICAL_AUTHOR_ROLES.iter().all(|required| {
        team.members
            .iter()
            .filter(|member| member.role == *required)
            .count()
            == 1
    })
}

fn require_canonical_author_role_contract(team: &ResearchTeam) -> Result<(), ApiError> {
    if canonical_author_roles_are_enforced(team) {
        return Ok(());
    }
    Err(ApiError::conflict(
        "team_role_contract_required",
        "team mutations require exactly one captain, evidence, and experiment role; any additional seats must be support",
    ))
}

fn require_author_role(
    team: &ResearchTeam,
    player_id: Uuid,
    required_role: &'static str,
    duty: &'static str,
) -> Result<(), ApiError> {
    // Legacy rosters remain readable, but can no longer enter the permissive
    // mutation path. A new canonical roster must be formed instead.
    require_canonical_author_role_contract(team)?;
    let actual_role = team
        .members
        .iter()
        .find(|member| member.player_id == player_id)
        .map(|member| member.role.as_str())
        .ok_or_else(|| {
            ApiError::forbidden(
                "user_not_on_team",
                "asserted human player is not a member of the research team",
            )
        })?;
    if actual_role == required_role {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "author_role_duty_required",
        format!(
            "{duty} is assigned to the {required_role} role; asserted player holds {actual_role}"
        ),
    ))
}

fn require_captain_or_self_assignment(
    team: &ResearchTeam,
    player_id: Uuid,
    assigned_player_id: Option<Uuid>,
) -> Result<(), ApiError> {
    require_canonical_author_role_contract(team)?;
    if assigned_player_id.is_some_and(|assigned| assigned == player_id) {
        return Ok(());
    }
    require_author_role(
        team,
        player_id,
        "captain",
        "cross-player work-item orchestration",
    )
}

fn require_captain_or_work_item_assignee(
    team: &ResearchTeam,
    player_id: Uuid,
    assigned_player_id: Option<Uuid>,
) -> Result<(), ApiError> {
    require_canonical_author_role_contract(team)?;
    if assigned_player_id.is_some_and(|assigned| assigned == player_id) {
        return Ok(());
    }
    require_author_role(
        team,
        player_id,
        "captain",
        "unassigned or cross-player work-item transition",
    )
}

async fn assert_team_actor_postgres(
    tx: &mut Transaction<'_, Postgres>,
    team_id: Uuid,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<(), ApiError> {
    let row = sqlx::query(
        "select p.record_json
         from hepta_research_team_members m
         join hepta_human_players p on p.player_id = m.player_id
         where m.team_id = $1 and m.player_id = $2
         for share of m, p",
    )
    .bind(team_id)
    .bind(assertion.player_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::forbidden(
            "user_not_on_team",
            "asserted human player is not a member of the research team",
        )
    })?;
    let player: HumanPlayer = decode_record(row.get("record_json"), "human player")?;
    assert_player_identity(assertion, &player)
}

fn memory_idempotency_key(operation: &str, idempotency_key: &str) -> String {
    format!("{operation}\n{idempotency_key}")
}

fn memory_replay<T: DeserializeOwned>(
    memory: &PaperRaidMemory,
    operation: &str,
    idempotency_key: &str,
    expected_request_hash: &str,
) -> Result<Option<(StatusCode, Json<T>)>, ApiError> {
    let Some(existing) = memory
        .idempotency
        .get(&memory_idempotency_key(operation, idempotency_key))
    else {
        return Ok(None);
    };
    if existing.request_hash != expected_request_hash {
        return Err(ApiError::conflict(
            "idempotency_key_conflict",
            "idempotency_key was already used with a different request",
        ));
    }
    let response = serde_json::from_value(existing.response.clone())
        .map_err(|error| ApiError::internal(format!("decode idempotent response: {error}")))?;
    Ok(Some((existing.status, Json(response))))
}

fn memory_remember<T: Serialize>(
    memory: &mut PaperRaidMemory,
    operation: &str,
    idempotency_key: &str,
    request_hash: String,
    status: StatusCode,
    response: &T,
) -> Result<(), ApiError> {
    memory.idempotency.insert(
        memory_idempotency_key(operation, idempotency_key),
        MemoryIdempotencyRecord {
            request_hash,
            status,
            response: serde_json::to_value(response).map_err(|error| {
                ApiError::internal(format!("encode idempotent response: {error}"))
            })?,
        },
    );
    Ok(())
}

#[derive(Debug)]
struct StoredResponse {
    status: StatusCode,
    response: Value,
}

async fn begin_postgres_idempotent<'a>(
    state: &'a AppState,
    operation: &str,
    idempotency_key: &str,
    expected_request_hash: &str,
) -> Result<(Transaction<'a, Postgres>, Option<StoredResponse>), ApiError> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| ApiError::internal("PostgreSQL state is unavailable"))?;
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let lock_key = format!("hepta-paper-raid:{operation}:{idempotency_key}");
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(lock_key)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let existing = sqlx::query(
        "select request_hash, response_status, response_json
         from hepta_paper_raid_idempotency
         where operation = $1 and idempotency_key = $2",
    )
    .bind(operation)
    .bind(idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let replay = if let Some(row) = existing {
        let request_hash: String = row.get("request_hash");
        if request_hash != expected_request_hash {
            return Err(ApiError::conflict(
                "idempotency_key_conflict",
                "idempotency_key was already used with a different request",
            ));
        }
        let response_status: i32 = row.get("response_status");
        let status = StatusCode::from_u16(
            u16::try_from(response_status)
                .map_err(|_| ApiError::internal("stored response status is invalid"))?,
        )
        .map_err(|_| ApiError::internal("stored response status is invalid"))?;
        Some(StoredResponse {
            status,
            response: row.get("response_json"),
        })
    } else {
        None
    };
    Ok((tx, replay))
}

fn decode_stored<T: DeserializeOwned>(
    stored: StoredResponse,
) -> Result<(StatusCode, Json<T>), ApiError> {
    let response = serde_json::from_value(stored.response)
        .map_err(|error| ApiError::internal(format!("decode idempotent response: {error}")))?;
    Ok((stored.status, Json(response)))
}

async fn finish_postgres_idempotent<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
    aggregate_id: Option<Uuid>,
    status: StatusCode,
    response: &T,
) -> Result<(), ApiError> {
    let response_json = serde_json::to_value(response)
        .map_err(|error| ApiError::internal(format!("encode idempotent response: {error}")))?;
    sqlx::query(
        "insert into hepta_paper_raid_idempotency (
            operation, idempotency_key, request_hash, aggregate_id,
            response_status, response_json
         ) values ($1,$2,$3,$4,$5,$6::jsonb)",
    )
    .bind(operation)
    .bind(idempotency_key)
    .bind(request_hash)
    .bind(aggregate_id)
    .bind(i32::from(status.as_u16()))
    .bind(response_json)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

async fn insert_postgres_event(
    tx: &mut Transaction<'_, Postgres>,
    operation: &str,
    idempotency_key: &str,
    event_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
) -> Result<(), ApiError> {
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|error| ApiError::internal(format!("encode Paper Raid event: {error}")))?;
    sqlx::query(
        "insert into hepta_outbox (
            event_id, event_type, aggregate_id, aggregate_version,
            correlation_id, causation_id, idempotency_key, schema_version,
            producer, payload_hash, payload, occurred_at
         ) values ($1,$2,$3,$4,$5,null,$6,$7,$8,$9,$10::jsonb,$11)",
    )
    .bind(Uuid::new_v4())
    .bind(event_type)
    .bind(aggregate_id.to_string())
    .bind(i64::try_from(aggregate_version).map_err(|_| ApiError::internal("version overflow"))?)
    .bind(Uuid::new_v4())
    .bind(format!("paper-raid:{operation}:{idempotency_key}"))
    .bind(PAPER_RAID_EVENT_SCHEMA_V2)
    .bind("hepta-research-league")
    .bind(sha256_digest(&payload_bytes))
    .bind(payload)
    .bind(Utc::now())
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    Ok(())
}

fn push_memory_event(
    memory: &mut PaperRaidMemory,
    operation: &str,
    idempotency_key: &str,
    event_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: Value,
) -> Result<(), ApiError> {
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|error| ApiError::internal(format!("encode Paper Raid event: {error}")))?;
    memory.events.push(EventEnvelope {
        schema_version: PAPER_RAID_EVENT_SCHEMA_V2.to_string(),
        event_id: Uuid::new_v4(),
        event_type: event_type.to_string(),
        aggregate_id: aggregate_id.to_string(),
        aggregate_version,
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        idempotency_key: format!("paper-raid:{operation}:{idempotency_key}"),
        occurred_at: Utc::now(),
        producer: "hepta-research-league".to_string(),
        payload_hash: sha256_digest(&payload_bytes),
        payload,
    });
    Ok(())
}

fn decode_record<T: DeserializeOwned>(value: Value, kind: &str) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|error| ApiError::internal(format!("decode stored {kind}: {error}")))
}

fn validate_team_members(members: &[CreateTeamMemberRequest]) -> Result<(), ApiError> {
    if !(3..=5).contains(&members.len()) {
        return Err(ApiError::bad_request(
            "invalid_team_size",
            "Paper Raid teams must contain 3 to 5 members",
        ));
    }
    let mut ordered = members.to_vec();
    ordered.sort_by_key(|member| member.participant_slot);
    let mut players = HashSet::new();
    let mut bindings = HashSet::new();
    let mut canonical_role_counts = HashMap::new();
    for (index, member) in ordered.iter().enumerate() {
        let expected_slot = u32::try_from(index + 1).expect("five slots fit u32");
        if member.participant_slot != expected_slot {
            return Err(ApiError::bad_request(
                "invalid_roster_slots",
                "participant slots must be unique and gapless from 1",
            ));
        }
        validate_contract_text_api("role", &member.role)?;
        if member.role != SUPPORT_AUTHOR_ROLE
            && !CANONICAL_AUTHOR_ROLES.contains(&member.role.as_str())
        {
            return Err(ApiError::bad_request(
                "invalid_team_role_contract",
                "team roles are restricted to captain, evidence, experiment, and support",
            ));
        }
        if member.role != SUPPORT_AUTHOR_ROLE {
            *canonical_role_counts
                .entry(member.role.as_str())
                .or_insert(0_u8) += 1;
        }
        if !players.insert(member.player_id) || !bindings.insert(member.binding_id) {
            return Err(ApiError::bad_request(
                "duplicate_team_member",
                "team players and Agent bindings must be unique",
            ));
        }
    }
    if CANONICAL_AUTHOR_ROLES
        .iter()
        .any(|role| canonical_role_counts.get(role).copied() != Some(1))
    {
        return Err(ApiError::bad_request(
            "invalid_team_role_contract",
            "team roster requires exactly one captain, evidence, and experiment role",
        ));
    }
    Ok(())
}

fn validate_new_research_team_request(request: &CreateResearchTeamRequest) -> Result<(), ApiError> {
    if request.team_id.as_bytes()[6] >> 4 != 4 {
        return Err(ApiError::bad_request(
            "generic_team_id_namespace_reserved",
            "direct team creation requires a UUIDv4 team_id; deterministic UUIDv5 IDs are reserved for matchmaking",
        ));
    }
    validate_team_members(&request.members)?;
    validate_digest_v2(
        "collaboration_compact_hash",
        &request.collaboration_compact_hash,
    )
}

async fn get_self_player(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<HumanPlayer>, ApiError> {
    let assertion = require_member_read_assertion(
        &headers,
        &state,
        "get_self_human_player_v2",
        "/v2/hepta/players/me",
    )?;
    let player = if let Some(pool) = &state.pool {
        let row = sqlx::query("select record_json from hepta_human_players where player_id=$1")
            .bind(assertion.player_id)
            .fetch_optional(pool)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("human_player_not_found", "human player does not exist")
            })?;
        decode_record(row.get("record_json"), "human player")?
    } else {
        state
            .paper_raid
            .read()
            .await
            .players
            .get(&assertion.player_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("human_player_not_found", "human player does not exist")
            })?
    };
    assert_player_identity(&assertion, &player)?;
    Ok(Json(player))
}

async fn list_self_agent_bindings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AgentBinding>>, ApiError> {
    let assertion = require_member_read_assertion(
        &headers,
        &state,
        "list_self_agent_bindings_v2",
        "/v2/hepta/agent-bindings",
    )?;
    let mut bindings = if let Some(pool) = &state.pool {
        let player_row =
            sqlx::query("select record_json from hepta_human_players where player_id=$1")
                .bind(assertion.player_id)
                .fetch_optional(pool)
                .await
                .map_err(ApiError::database)?
                .ok_or_else(|| {
                    ApiError::not_found("human_player_not_found", "human player does not exist")
                })?;
        let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
        assert_player_identity(&assertion, &player)?;
        sqlx::query(
            "select record_json from hepta_agent_bindings
             where player_id=$1 order by created_at,binding_id",
        )
        .bind(assertion.player_id)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "Agent binding"))
        .collect::<Result<Vec<_>, _>>()?
    } else {
        let memory = state.paper_raid.read().await;
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::not_found("human_player_not_found", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        memory
            .bindings
            .values()
            .filter(|binding| binding.player_id == assertion.player_id)
            .cloned()
            .collect()
    };
    bindings.sort_by_key(|binding| (binding.created_at, binding.binding_id));
    Ok(Json(bindings))
}

async fn create_player(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateHumanPlayerRequest>,
) -> Result<(StatusCode, Json<HumanPlayer>), ApiError> {
    const OPERATION: &str = "create_human_player_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contract_text_api("display_name", &request.display_name)?;
    validate_contract_text_api("signing_key_id", &request.signing_key_id)?;
    let signing_public_key = canonical_public_key(&request.signing_public_key)?;
    let signing_public_key_hash = sha256_digest(
        &BASE64
            .decode(&signing_public_key)
            .map_err(|_| ApiError::internal("canonical public key decode failed"))?,
    );
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion_for_applied_replay(
        &headers,
        &state,
        OPERATION,
        "POST",
        "/v2/hepta/players",
        &request.idempotency_key,
        &request_hash,
    )?;
    if assertion.player_id != request.player_id {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "asserted player_id does not match the player being created",
        ));
    }
    let now = Utc::now();
    let registration = HumanKeyRegistrationClaimV2 {
        schema: HUMAN_KEY_REGISTRATION_V2.to_string(),
        player_id: request.player_id,
        subject_id: assertion.subject_id.clone(),
        nakama_user_id: assertion.nakama_user_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: signing_public_key.clone(),
        signing_public_key_hash: signing_public_key_hash.clone(),
        nonce: request.idempotency_key.clone(),
        issued_at_unix: request.key_issued_at_unix,
        expires_at_unix: request.key_expires_at_unix,
    };
    verify_human_key_registration_pop(&registration, &request.key_proof_signature)
        .map_err(|message| ApiError::forbidden("invalid_human_key_registration", message))?;
    let player = HumanPlayer {
        player_id: request.player_id,
        subject_id: assertion.subject_id.clone(),
        nakama_user_id: assertion.nakama_user_id,
        display_name: request.display_name.clone(),
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key,
        signing_public_key_hash,
        status: HumanPlayerStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let signing_key = HumanSigningKey {
        player_id: player.player_id,
        signing_key_id: player.signing_key_id.clone(),
        signing_public_key: player.signing_public_key.clone(),
        signing_public_key_hash: player.signing_public_key_hash.clone(),
        status: HumanSigningKeyStatus::Active,
        version: 1,
        registered_at: now,
        retired_at: None,
        revoked_at: None,
        revocation_reason_hash: None,
    };

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        enforce_user_assertion_time(&assertion)?;
        enforce_onboarding_proof_time(
            registration.issued_at_unix,
            registration.expires_at_unix,
            "invalid_human_key_registration",
            "human key proof must be active and valid for at most ten minutes",
        )?;
        if memory.players.contains_key(&player.player_id)
            || memory.players_by_subject.contains_key(&player.subject_id)
            || memory
                .players_by_nakama_user
                .contains_key(&player.nakama_user_id)
        {
            return Err(ApiError::conflict(
                "human_player_conflict",
                "player_id or subject_id already exists",
            ));
        }
        memory
            .players_by_subject
            .insert(player.subject_id.clone(), player.player_id);
        memory
            .players_by_nakama_user
            .insert(player.nakama_user_id, player.player_id);
        memory.players.insert(player.player_id, player.clone());
        memory.human_signing_keys.insert(
            (signing_key.player_id, signing_key.signing_key_id.clone()),
            signing_key.clone(),
        );
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.human_player.created.v2",
            player.player_id,
            player.version,
            json!({"player_id": player.player_id, "subject_id": player.subject_id}),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &player,
        )?;
        return Ok((StatusCode::CREATED, Json(player)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    enforce_user_assertion_time(&assertion)?;
    enforce_onboarding_proof_time(
        registration.issued_at_unix,
        registration.expires_at_unix,
        "invalid_human_key_registration",
        "human key proof must be active and valid for at most ten minutes",
    )?;
    let record_json = serde_json::to_value(&player)
        .map_err(|error| ApiError::internal(format!("encode human player: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_human_players (
            player_id, subject_id, nakama_user_id, signing_key_id, signing_public_key,
            signing_public_key_hash, status, version, record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9::jsonb,$10,$11)",
    )
    .bind(player.player_id)
    .bind(&player.subject_id)
    .bind(player.nakama_user_id)
    .bind(&player.signing_key_id)
    .bind(&player.signing_public_key)
    .bind(&player.signing_public_key_hash)
    .bind(player.status.as_str())
    .bind(player.version as i64)
    .bind(record_json)
    .bind(player.created_at)
    .bind(player.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "human_player_conflict",
                "player_id or subject_id already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    sqlx::query(
        "insert into hepta_human_signing_keys (
            player_id, signing_key_id, signing_public_key, signing_public_key_hash,
            status, version, registered_at, retired_at, revoked_at,
            revocation_reason_hash, record_json
         ) values ($1,$2,$3,$4,$5,$6,$7,null,null,null,$8::jsonb)",
    )
    .bind(signing_key.player_id)
    .bind(&signing_key.signing_key_id)
    .bind(&signing_key.signing_public_key)
    .bind(&signing_key.signing_public_key_hash)
    .bind(signing_key.status.as_str())
    .bind(signing_key.version as i64)
    .bind(signing_key.registered_at)
    .bind(
        serde_json::to_value(&signing_key)
            .map_err(|error| ApiError::internal(format!("encode human signing key: {error}")))?,
    )
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.human_player.created.v2",
        player.player_id,
        player.version,
        json!({"player_id": player.player_id, "subject_id": player.subject_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(player.player_id),
        StatusCode::CREATED,
        &player,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(player)))
}

fn validate_human_key_command_window(issued_at: i64, expires_at: i64) -> Result<(), ApiError> {
    let now = Utc::now().timestamp();
    if issued_at < 0
        || expires_at <= issued_at
        || expires_at - issued_at > 600
        || now < issued_at
        || now >= expires_at
    {
        return Err(ApiError::forbidden(
            "invalid_human_key_command",
            "human key command must be active and valid for at most ten minutes",
        ));
    }
    Ok(())
}

fn human_key_rotation_claim(
    player: &HumanPlayer,
    request: &RotateHumanSigningKeyRequest,
) -> Result<HumanKeyRotationClaimV2, ApiError> {
    let new_public_key = canonical_public_key(&request.new_signing_public_key)?;
    let new_key_hash = sha256_digest(
        &BASE64
            .decode(&new_public_key)
            .map_err(|_| ApiError::internal("canonical human key decode failed"))?,
    );
    Ok(HumanKeyRotationClaimV2 {
        schema: HUMAN_KEY_ROTATION_V2.to_string(),
        rotation_id: request.rotation_id,
        player_id: player.player_id,
        subject_id: player.subject_id.clone(),
        nakama_user_id: player.nakama_user_id,
        old_signing_key_id: player.signing_key_id.clone(),
        old_signing_public_key_hash: player.signing_public_key_hash.clone(),
        new_signing_key_id: request.new_signing_key_id.clone(),
        new_signing_public_key: new_public_key,
        new_signing_public_key_hash: new_key_hash,
        nonce: request.idempotency_key.clone(),
        issued_at_unix: request.issued_at_unix,
        expires_at_unix: request.expires_at_unix,
    })
}

async fn rotate_human_signing_key(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RotateHumanSigningKeyRequest>,
) -> Result<(StatusCode, Json<HumanPlayer>), ApiError> {
    const OPERATION: &str = "rotate_human_signing_key_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contract_text_api("new_signing_key_id", &request.new_signing_key_id)?;
    validate_human_key_command_window(request.issued_at_unix, request.expires_at_unix)?;
    let request_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/players/{player_id}/signing-key/rotate");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if assertion.player_id != player_id {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "asserted player differs from rotation target",
        ));
    }
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let snapshot = memory.players.get(&player_id).cloned().ok_or_else(|| {
            ApiError::not_found("human_player_not_found", "human player does not exist")
        })?;
        assert_player_identity(&assertion, &snapshot)?;
        if snapshot.version != request.expected_player_version
            || snapshot.status != HumanPlayerStatus::Active
        {
            return Err(version_conflict(
                "human player",
                request.expected_player_version,
                snapshot.version,
            ));
        }
        let claim = human_key_rotation_claim(&snapshot, &request)?;
        verify_human_key_rotation_signatures(
            &claim,
            &snapshot.signing_public_key,
            &request.old_key_signature,
            &request.new_key_signature,
        )
        .map_err(|message| ApiError::forbidden("invalid_human_key_rotation", message))?;
        if memory
            .human_signing_keys
            .contains_key(&(player_id, claim.new_signing_key_id.clone()))
        {
            return Err(ApiError::conflict(
                "human_signing_key_conflict",
                "new signing_key_id was already registered",
            ));
        }
        let old = memory
            .human_signing_keys
            .get_mut(&(player_id, snapshot.signing_key_id.clone()))
            .ok_or_else(|| ApiError::internal("active human signing key record is missing"))?;
        old.status = HumanSigningKeyStatus::Rotated;
        old.version += 1;
        old.retired_at = Some(now);
        let new_key = HumanSigningKey {
            player_id,
            signing_key_id: claim.new_signing_key_id.clone(),
            signing_public_key: claim.new_signing_public_key.clone(),
            signing_public_key_hash: claim.new_signing_public_key_hash.clone(),
            status: HumanSigningKeyStatus::Active,
            version: 1,
            registered_at: now,
            retired_at: None,
            revoked_at: None,
            revocation_reason_hash: None,
        };
        memory
            .human_signing_keys
            .insert((player_id, new_key.signing_key_id.clone()), new_key.clone());
        for acceptance in memory.team_acceptances.values_mut().filter(|acceptance| {
            acceptance.player_id == player_id && acceptance.superseded_at.is_none()
        }) {
            acceptance.superseded_at = Some(now);
        }
        let player = memory.players.get_mut(&player_id).expect("player exists");
        player.signing_key_id = new_key.signing_key_id;
        player.signing_public_key = new_key.signing_public_key;
        player.signing_public_key_hash = new_key.signing_public_key_hash;
        player.version += 1;
        player.updated_at = now;
        let response = player.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.human_signing_key.rotated.v2",
            player_id,
            response.version,
            json!({"player_id": player_id, "signing_key_id": response.signing_key_id}),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let row = sqlx::query(
        "select version, record_json from hepta_human_players where player_id = $1 for update",
    )
    .bind(player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("human_player_not_found", "human player does not exist"))?;
    let mut player: HumanPlayer = decode_record(row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("human player version is invalid"))?;
    if actual_version != request.expected_player_version
        || player.status != HumanPlayerStatus::Active
    {
        return Err(version_conflict(
            "human player",
            request.expected_player_version,
            actual_version,
        ));
    }
    let claim = human_key_rotation_claim(&player, &request)?;
    verify_human_key_rotation_signatures(
        &claim,
        &player.signing_public_key,
        &request.old_key_signature,
        &request.new_key_signature,
    )
    .map_err(|message| ApiError::forbidden("invalid_human_key_rotation", message))?;
    let old_updated = sqlx::query(
        "update hepta_human_signing_keys
         set status = 'rotated', version = version + 1, retired_at = $1,
             record_json = record_json || jsonb_build_object(
                 'status', 'rotated', 'version', version + 1, 'retired_at', $1::timestamptz)
         where player_id = $2 and signing_key_id = $3 and status = 'active'",
    )
    .bind(now)
    .bind(player_id)
    .bind(&player.signing_key_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if old_updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "human_signing_key_conflict",
            "active signing key changed concurrently",
        ));
    }
    sqlx::query(
        "update hepta_research_team_member_acceptances
         set superseded_at = $1,
             record_json = record_json || jsonb_build_object('superseded_at', $1::timestamptz)
         where player_id = $2 and superseded_at is null",
    )
    .bind(now)
    .bind(player_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let new_key = HumanSigningKey {
        player_id,
        signing_key_id: claim.new_signing_key_id,
        signing_public_key: claim.new_signing_public_key,
        signing_public_key_hash: claim.new_signing_public_key_hash,
        status: HumanSigningKeyStatus::Active,
        version: 1,
        registered_at: now,
        retired_at: None,
        revoked_at: None,
        revocation_reason_hash: None,
    };
    let key_json = serde_json::to_value(&new_key)
        .map_err(|error| ApiError::internal(format!("encode new human key: {error}")))?;
    sqlx::query(
        "insert into hepta_human_signing_keys (
            player_id, signing_key_id, signing_public_key, signing_public_key_hash,
            status, version, registered_at, retired_at, revoked_at,
            revocation_reason_hash, record_json
         ) values ($1,$2,$3,$4,'active',1,$5,null,null,null,$6::jsonb)",
    )
    .bind(player_id)
    .bind(&new_key.signing_key_id)
    .bind(&new_key.signing_public_key)
    .bind(&new_key.signing_public_key_hash)
    .bind(now)
    .bind(key_json)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    player.signing_key_id = new_key.signing_key_id;
    player.signing_public_key = new_key.signing_public_key;
    player.signing_public_key_hash = new_key.signing_public_key_hash;
    player.version = actual_version + 1;
    player.updated_at = now;
    let player_json = serde_json::to_value(&player)
        .map_err(|error| ApiError::internal(format!("encode rotated player: {error}")))?;
    let updated = sqlx::query(
        "update hepta_human_players
         set signing_key_id=$1, signing_public_key=$2, signing_public_key_hash=$3,
             version=$4, record_json=$5::jsonb, updated_at=$6
         where player_id=$7 and version=$8",
    )
    .bind(&player.signing_key_id)
    .bind(&player.signing_public_key)
    .bind(&player.signing_public_key_hash)
    .bind(player.version as i64)
    .bind(player_json)
    .bind(player.updated_at)
    .bind(player.player_id)
    .bind(request.expected_player_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "human player changed concurrently",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.human_signing_key.rotated.v2",
        player_id,
        player.version,
        json!({"player_id": player_id, "signing_key_id": player.signing_key_id}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(player_id),
        StatusCode::OK,
        &player,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(player)))
}

async fn revoke_human_signing_key(
    State(state): State<AppState>,
    Path(player_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RevokeHumanSigningKeyRequest>,
) -> Result<(StatusCode, Json<HumanPlayer>), ApiError> {
    const OPERATION: &str = "revoke_human_signing_key_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("reason_hash", &request.reason_hash)?;
    validate_human_key_command_window(request.issued_at_unix, request.expires_at_unix)?;
    let request_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/players/{player_id}/signing-key/revoke");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if assertion.player_id != player_id {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "asserted player differs from revocation target",
        ));
    }
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let snapshot = memory.players.get(&player_id).cloned().ok_or_else(|| {
            ApiError::not_found("human_player_not_found", "human player does not exist")
        })?;
        assert_player_identity(&assertion, &snapshot)?;
        if snapshot.version != request.expected_player_version
            || snapshot.status != HumanPlayerStatus::Active
        {
            return Err(version_conflict(
                "human player",
                request.expected_player_version,
                snapshot.version,
            ));
        }
        let claim = HumanKeyRevocationClaimV2 {
            schema: HUMAN_KEY_REVOCATION_V2.to_string(),
            revocation_id: request.revocation_id,
            player_id,
            subject_id: snapshot.subject_id.clone(),
            nakama_user_id: snapshot.nakama_user_id,
            signing_key_id: snapshot.signing_key_id.clone(),
            signing_public_key_hash: snapshot.signing_public_key_hash.clone(),
            reason_hash: request.reason_hash.clone(),
            nonce: request.idempotency_key.clone(),
            issued_at_unix: request.issued_at_unix,
            expires_at_unix: request.expires_at_unix,
        };
        verify_human_key_revocation_signature(
            &claim,
            &snapshot.signing_public_key,
            &request.signature,
        )
        .map_err(|message| ApiError::forbidden("invalid_human_key_revocation", message))?;
        let key = memory
            .human_signing_keys
            .get_mut(&(player_id, snapshot.signing_key_id.clone()))
            .ok_or_else(|| ApiError::internal("active human signing key record is missing"))?;
        key.status = HumanSigningKeyStatus::Revoked;
        key.version += 1;
        key.retired_at = Some(now);
        key.revoked_at = Some(now);
        key.revocation_reason_hash = Some(request.reason_hash.clone());
        for acceptance in memory.team_acceptances.values_mut().filter(|acceptance| {
            acceptance.player_id == player_id && acceptance.superseded_at.is_none()
        }) {
            acceptance.superseded_at = Some(now);
        }
        let player = memory.players.get_mut(&player_id).expect("player exists");
        player.status = HumanPlayerStatus::Suspended;
        player.version += 1;
        player.updated_at = now;
        let response = player.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.human_signing_key.revoked.v2",
            player_id,
            response.version,
            json!({"player_id": player_id, "reason_hash": request.reason_hash}),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let row = sqlx::query(
        "select version, record_json from hepta_human_players where player_id = $1 for update",
    )
    .bind(player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("human_player_not_found", "human player does not exist"))?;
    let mut player: HumanPlayer = decode_record(row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("human player version is invalid"))?;
    if actual_version != request.expected_player_version
        || player.status != HumanPlayerStatus::Active
    {
        return Err(version_conflict(
            "human player",
            request.expected_player_version,
            actual_version,
        ));
    }
    let claim = HumanKeyRevocationClaimV2 {
        schema: HUMAN_KEY_REVOCATION_V2.to_string(),
        revocation_id: request.revocation_id,
        player_id,
        subject_id: player.subject_id.clone(),
        nakama_user_id: player.nakama_user_id,
        signing_key_id: player.signing_key_id.clone(),
        signing_public_key_hash: player.signing_public_key_hash.clone(),
        reason_hash: request.reason_hash.clone(),
        nonce: request.idempotency_key.clone(),
        issued_at_unix: request.issued_at_unix,
        expires_at_unix: request.expires_at_unix,
    };
    verify_human_key_revocation_signature(&claim, &player.signing_public_key, &request.signature)
        .map_err(|message| ApiError::forbidden("invalid_human_key_revocation", message))?;
    let key_updated = sqlx::query(
        "update hepta_human_signing_keys
         set status='revoked', version=version+1, retired_at=$1,
             revoked_at=$1, revocation_reason_hash=$2,
             record_json=record_json || jsonb_build_object(
                 'status','revoked','version',version+1,
                 'retired_at',$1::timestamptz,'revoked_at',$1::timestamptz,
                 'revocation_reason_hash',$2::text)
         where player_id=$3 and signing_key_id=$4 and status='active'",
    )
    .bind(now)
    .bind(&request.reason_hash)
    .bind(player_id)
    .bind(&player.signing_key_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if key_updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "human_signing_key_conflict",
            "active signing key changed concurrently",
        ));
    }
    sqlx::query(
        "update hepta_research_team_member_acceptances
         set superseded_at = $1,
             record_json = record_json || jsonb_build_object('superseded_at', $1::timestamptz)
         where player_id = $2 and superseded_at is null",
    )
    .bind(now)
    .bind(player_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    player.status = HumanPlayerStatus::Suspended;
    player.version = actual_version + 1;
    player.updated_at = now;
    let player_json = serde_json::to_value(&player)
        .map_err(|error| ApiError::internal(format!("encode revoked player: {error}")))?;
    let updated = sqlx::query(
        "update hepta_human_players set status='suspended', version=$1,
             record_json=$2::jsonb, updated_at=$3 where player_id=$4 and version=$5",
    )
    .bind(player.version as i64)
    .bind(player_json)
    .bind(player.updated_at)
    .bind(player_id)
    .bind(request.expected_player_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "human player changed concurrently",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.human_signing_key.revoked.v2",
        player_id,
        player.version,
        json!({"player_id": player_id, "reason_hash": request.reason_hash}),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(player_id),
        StatusCode::OK,
        &player,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(player)))
}

async fn create_agent_binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateAgentBindingRequest>,
) -> Result<(StatusCode, Json<AgentBinding>), ApiError> {
    const OPERATION_V2: &str = "create_agent_binding_v2";
    const OPERATION_V3: &str = "create_agent_binding_v3";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_non_empty("agent_id", &request.agent_id)?;
    validate_contract_text_api("agent_key_id", &request.agent_key_id)?;
    validate_contract_text_api("agent_proof_nonce", &request.agent_proof_nonce)?;
    if request.agent_proof_nonce != request.idempotency_key {
        return Err(ApiError::bad_request(
            "agent_binding_nonce_scope_mismatch",
            "Agent proof nonce must equal the request idempotency key",
        ));
    }
    let agent_public_key = canonical_public_key(&request.agent_public_key)?;
    let agent_public_key_bytes = BASE64
        .decode(&agent_public_key)
        .map_err(|_| ApiError::internal("canonical Agent public key decode failed"))?;
    let agent_public_key_hash = sha256_digest(&agent_public_key_bytes);
    if request.agent_key_id != agent_public_key_hash {
        return Err(ApiError::bad_request(
            "agent_key_id_mismatch",
            "agent_key_id must equal the canonical Agent public-key hash",
        ));
    }
    let capability_disclosure = validate_agent_binding_capability_disclosure(&request)?;
    let operation = if capability_disclosure.is_some() {
        OPERATION_V3
    } else {
        OPERATION_V2
    };
    let event_type = if capability_disclosure.is_some() {
        "hepta.paper_raid.agent_binding.created.v3"
    } else {
        "hepta.paper_raid.agent_binding.created.v2"
    };
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion_for_applied_replay(
        &headers,
        &state,
        operation,
        "POST",
        "/v2/hepta/agent-bindings",
        &request.idempotency_key,
        &request_hash,
    )?;
    if assertion.player_id != request.player_id {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "asserted player_id does not match the Agent binding owner",
        ));
    }
    let now = Utc::now();
    if let Some((_, disclosure_hash)) = &capability_disclosure {
        let proof = AgentBindingProofClaimV3 {
            schema: AGENT_BINDING_PROOF_V3.to_string(),
            binding_id: request.binding_id,
            agent_id: request.agent_id.clone(),
            agent_key_id: request.agent_key_id.clone(),
            agent_public_key: agent_public_key.clone(),
            agent_public_key_hash: agent_public_key_hash.clone(),
            capability_disclosure_hash: disclosure_hash.clone(),
            subject_id: assertion.subject_id.clone(),
            player_id: request.player_id,
            nonce: request.agent_proof_nonce.clone(),
            issued_at_unix: request.agent_proof_issued_at_unix,
            expires_at_unix: request.agent_proof_expires_at_unix,
        };
        verify_agent_binding_proof_v3(&proof, &request.agent_proof_signature)
            .map_err(|message| ApiError::forbidden("invalid_agent_binding_proof", message))?;
    } else {
        let proof = AgentBindingProofClaimV2 {
            schema: AGENT_BINDING_PROOF_V2.to_string(),
            binding_id: request.binding_id,
            agent_id: request.agent_id.clone(),
            agent_key_id: request.agent_key_id.clone(),
            agent_public_key: agent_public_key.clone(),
            agent_public_key_hash: agent_public_key_hash.clone(),
            subject_id: assertion.subject_id.clone(),
            player_id: request.player_id,
            nonce: request.agent_proof_nonce.clone(),
            issued_at_unix: request.agent_proof_issued_at_unix,
            expires_at_unix: request.agent_proof_expires_at_unix,
        };
        verify_agent_binding_proof(&proof, &request.agent_proof_signature)
            .map_err(|message| ApiError::forbidden("invalid_agent_binding_proof", message))?;
    }
    let (capability_disclosure, capability_disclosure_hash) = capability_disclosure
        .map(|(disclosure, hash)| (Some(disclosure), Some(hash)))
        .unwrap_or((None, None));
    let binding = AgentBinding {
        binding_id: request.binding_id,
        player_id: request.player_id,
        agent_id: request.agent_id.clone(),
        agent_key_id: request.agent_key_id.clone(),
        agent_public_key,
        agent_public_key_hash,
        capability_disclosure,
        capability_disclosure_hash,
        status: AgentBindingStatus::Active,
        version: 1,
        created_at: now,
        updated_at: now,
    };

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, operation, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        enforce_user_assertion_time(&assertion)?;
        enforce_onboarding_proof_time(
            request.agent_proof_issued_at_unix,
            request.agent_proof_expires_at_unix,
            "invalid_agent_binding_proof",
            "Agent binding proof must be active and valid for at most ten minutes",
        )?;
        let player = memory.players.get(&request.player_id).ok_or_else(|| {
            ApiError::not_found("human_player_not_found", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        if player.status != HumanPlayerStatus::Active {
            return Err(ApiError::forbidden(
                "human_player_inactive",
                "Agent binding requires an active human player",
            ));
        }
        if memory
            .used_agent_binding_nonces
            .contains(&(binding.agent_id.clone(), request.agent_proof_nonce.clone()))
        {
            return Err(ApiError::conflict(
                "agent_binding_nonce_reused",
                "Agent binding proof nonce has already been accepted",
            ));
        }
        if memory.bindings.contains_key(&binding.binding_id)
            || memory
                .active_bindings_by_agent
                .contains_key(&binding.agent_id)
        {
            return Err(ApiError::conflict(
                "agent_binding_conflict",
                "binding_id or active agent binding already exists",
            ));
        }
        memory
            .active_bindings_by_agent
            .insert(binding.agent_id.clone(), binding.binding_id);
        memory
            .used_agent_binding_nonces
            .insert((binding.agent_id.clone(), request.agent_proof_nonce.clone()));
        memory.bindings.insert(binding.binding_id, binding.clone());
        push_memory_event(
            &mut memory,
            operation,
            &request.idempotency_key,
            event_type,
            binding.binding_id,
            binding.version,
            agent_binding_created_event_payload(&binding),
        )?;
        memory_remember(
            &mut memory,
            operation,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &binding,
        )?;
        return Ok((StatusCode::CREATED, Json(binding)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, operation, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    enforce_user_assertion_time(&assertion)?;
    enforce_onboarding_proof_time(
        request.agent_proof_issued_at_unix,
        request.agent_proof_expires_at_unix,
        "invalid_agent_binding_proof",
        "Agent binding proof must be active and valid for at most ten minutes",
    )?;
    let player_row = sqlx::query(
        "select status, record_json from hepta_human_players where player_id = $1 for update",
    )
    .bind(binding.player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("human_player_not_found", "human player does not exist"))?;
    let status: String = player_row.get("status");
    let stored_player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &stored_player)?;
    if status != "active" {
        return Err(ApiError::forbidden(
            "human_player_inactive",
            "Agent binding requires an active human player",
        ));
    }
    let record_json = serde_json::to_value(&binding)
        .map_err(|error| ApiError::internal(format!("encode Agent binding: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_agent_bindings (
            binding_id, player_id, agent_id, status, version,
            capability_disclosure_hash, record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9)",
    )
    .bind(binding.binding_id)
    .bind(binding.player_id)
    .bind(&binding.agent_id)
    .bind(binding.status.as_str())
    .bind(binding.version as i64)
    .bind(binding.capability_disclosure_hash.as_deref())
    .bind(record_json)
    .bind(binding.created_at)
    .bind(binding.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "agent_binding_conflict",
                "binding_id or active agent binding already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    let nonce_insert = sqlx::query(
        "insert into hepta_agent_binding_nonces (agent_id, nonce, binding_id)
         values ($1,$2,$3)",
    )
    .bind(&binding.agent_id)
    .bind(&request.agent_proof_nonce)
    .bind(binding.binding_id)
    .execute(&mut *tx)
    .await;
    if let Err(error) = nonce_insert {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "agent_binding_nonce_reused",
                "Agent binding proof nonce has already been accepted",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_postgres_event(
        &mut tx,
        operation,
        &request.idempotency_key,
        event_type,
        binding.binding_id,
        binding.version,
        agent_binding_created_event_payload(&binding),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        operation,
        &request.idempotency_key,
        &request_hash,
        Some(binding.binding_id),
        StatusCode::CREATED,
        &binding,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(binding)))
}

fn validate_agent_binding_capability_disclosure(
    request: &CreateAgentBindingRequest,
) -> Result<Option<(AgentCapabilityDisclosureV1, String)>, ApiError> {
    match (
        request.agent_proof_schema.as_deref(),
        request.capability_disclosure.as_ref(),
        request.capability_disclosure_hash.as_deref(),
    ) {
        (None, None, None) | (Some(AGENT_BINDING_PROOF_V2), None, None) => Ok(None),
        (Some(AGENT_BINDING_PROOF_V3), Some(disclosure), Some(declared_hash)) => {
            let calculated_hash =
                agent_capability_disclosure_hash(disclosure).map_err(|message| {
                    ApiError::bad_request("invalid_agent_capability_disclosure", message)
                })?;
            if calculated_hash != declared_hash {
                return Err(ApiError::bad_request(
                    "agent_capability_disclosure_hash_mismatch",
                    "capability_disclosure_hash does not match the canonical disclosure frame",
                ));
            }
            Ok(Some((disclosure.clone(), calculated_hash)))
        }
        (Some(schema), _, _)
            if schema != AGENT_BINDING_PROOF_V2 && schema != AGENT_BINDING_PROOF_V3 =>
        {
            Err(ApiError::bad_request(
                "unsupported_agent_binding_proof_schema",
                "agent_proof_schema is not supported",
            ))
        }
        _ => Err(ApiError::bad_request(
            "agent_binding_proof_version_mismatch",
            "V2 forbids capability fields and V3 requires the exact disclosure and hash",
        )),
    }
}

fn agent_binding_created_event_payload(binding: &AgentBinding) -> Value {
    match binding.capability_disclosure_hash.as_deref() {
        Some(disclosure_hash) => json!({
            "binding_id": binding.binding_id,
            "player_id": binding.player_id,
            "agent_id": binding.agent_id,
            "capability_disclosure_hash": disclosure_hash,
            "capability_assurance": "self_declared_unverified",
        }),
        None => json!({
            "binding_id": binding.binding_id,
            "player_id": binding.player_id,
            "agent_id": binding.agent_id,
        }),
    }
}

async fn rotate_agent_binding_key(
    State(state): State<AppState>,
    Path(binding_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<RotateAgentBindingKeyRequest>,
) -> Result<(StatusCode, Json<AgentBinding>), ApiError> {
    const OPERATION: &str = "rotate_agent_binding_key_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    if request.expected_binding_version == 0 || request.expected_binding_version > JSON_SAFE_U64_MAX
    {
        return Err(ApiError::bad_request(
            "invalid_expected_version",
            "expected_binding_version must be a positive JSON-safe integer",
        ));
    }
    validate_non_empty("agent_id", &request.agent_id)?;
    validate_contract_text_api("old_agent_key_id", &request.old_agent_key_id)?;
    validate_contract_text_api("new_agent_key_id", &request.new_agent_key_id)?;
    let old_agent_public_key = canonical_public_key(&request.old_agent_public_key)?;
    let new_agent_public_key = canonical_public_key(&request.new_agent_public_key)?;
    let old_agent_public_key_hash = sha256_digest(
        &BASE64
            .decode(&old_agent_public_key)
            .map_err(|_| ApiError::internal("canonical old Agent public key decode failed"))?,
    );
    let new_agent_public_key_hash = sha256_digest(
        &BASE64
            .decode(&new_agent_public_key)
            .map_err(|_| ApiError::internal("canonical new Agent public key decode failed"))?,
    );
    if request.old_agent_key_id != old_agent_public_key_hash
        || request.new_agent_key_id != new_agent_public_key_hash
    {
        return Err(ApiError::bad_request(
            "agent_key_id_mismatch",
            "old and new Agent key IDs must equal their canonical public-key hashes",
        ));
    }
    let request_hash = request_hash(&request)?;
    let path = format!("/v2/hepta/agent-bindings/{binding_id}/rotate-key");
    let assertion = require_user_assertion_for_applied_replay(
        &headers,
        &state,
        OPERATION,
        "POST",
        &path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let claim = AgentBindingKeyRotationClaimV2 {
        schema: AGENT_BINDING_KEY_ROTATION_V2.to_string(),
        rotation_id: request.rotation_id,
        binding_id,
        expected_binding_version: request.expected_binding_version,
        player_id: assertion.player_id,
        subject_id: assertion.subject_id.clone(),
        agent_id: request.agent_id.clone(),
        old_agent_key_id: request.old_agent_key_id.clone(),
        old_agent_public_key: old_agent_public_key.clone(),
        old_agent_public_key_hash: old_agent_public_key_hash.clone(),
        new_agent_key_id: request.new_agent_key_id.clone(),
        new_agent_public_key: new_agent_public_key.clone(),
        new_agent_public_key_hash: new_agent_public_key_hash.clone(),
        nonce: request.idempotency_key.clone(),
        issued_at_unix: request.issued_at_unix,
        expires_at_unix: request.expires_at_unix,
    };
    verify_agent_binding_key_rotation_signatures(
        &claim,
        &request.old_key_signature,
        &request.new_key_signature,
    )
    .map_err(|message| ApiError::forbidden("invalid_agent_binding_key_rotation", message))?;
    let now = Utc::now();
    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        enforce_user_assertion_time(&assertion)?;
        enforce_onboarding_proof_time(
            claim.issued_at_unix,
            claim.expires_at_unix,
            "invalid_agent_binding_key_rotation",
            "Agent key rotation must be active and valid for at most ten minutes",
        )?;
        let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
            ApiError::not_found("human_player_not_found", "human player does not exist")
        })?;
        assert_player_identity(&assertion, player)?;
        let snapshot = memory.bindings.get(&binding_id).cloned().ok_or_else(|| {
            ApiError::not_found("agent_binding_not_found", "Agent binding does not exist")
        })?;
        validate_agent_binding_rotation_scope(&snapshot, &claim)?;
        if memory
            .used_agent_binding_rotation_ids
            .contains(&request.rotation_id)
            || memory
                .used_agent_binding_rotation_nonces
                .contains(&(binding_id, request.idempotency_key.clone()))
        {
            return Err(ApiError::conflict(
                "agent_binding_rotation_reused",
                "Agent binding key rotation ID or nonce has already been accepted",
            ));
        }
        let binding = memory
            .bindings
            .get_mut(&binding_id)
            .expect("binding exists");
        binding.agent_key_id = new_agent_public_key_hash.clone();
        binding.agent_public_key = new_agent_public_key;
        binding.agent_public_key_hash = new_agent_public_key_hash;
        binding.version += 1;
        binding.updated_at = now;
        let response = binding.clone();
        memory
            .used_agent_binding_rotation_nonces
            .insert((binding_id, request.idempotency_key.clone()));
        memory
            .used_agent_binding_rotation_ids
            .insert(request.rotation_id);
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.agent_binding.key_rotated.v2",
            binding_id,
            response.version,
            json!({
                "rotation_id": request.rotation_id,
                "binding_id": binding_id,
                "agent_id": response.agent_id,
                "old_agent_key_id": claim.old_agent_key_id,
                "new_agent_key_id": response.agent_key_id,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    enforce_user_assertion_time(&assertion)?;
    enforce_onboarding_proof_time(
        claim.issued_at_unix,
        claim.expires_at_unix,
        "invalid_agent_binding_key_rotation",
        "Agent key rotation must be active and valid for at most ten minutes",
    )?;
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id=$1 for share")
            .bind(assertion.player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("human_player_not_found", "human player does not exist")
            })?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let binding_row = sqlx::query(
        "select version, record_json from hepta_agent_bindings where binding_id=$1 for update",
    )
    .bind(binding_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("agent_binding_not_found", "Agent binding does not exist")
    })?;
    let mut binding: AgentBinding = decode_record(binding_row.get("record_json"), "Agent binding")?;
    let actual_version = u64::try_from(binding_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("Agent binding version is invalid"))?;
    if binding.version != actual_version {
        return Err(ApiError::internal(
            "Agent binding row and canonical record version differ",
        ));
    }
    validate_agent_binding_rotation_scope(&binding, &claim)?;
    binding.agent_key_id = new_agent_public_key_hash.clone();
    binding.agent_public_key = new_agent_public_key;
    binding.agent_public_key_hash = new_agent_public_key_hash;
    binding.version += 1;
    binding.updated_at = now;
    let record_json = serde_json::to_value(&binding)
        .map_err(|error| ApiError::internal(format!("encode Agent binding: {error}")))?;
    let updated = sqlx::query(
        "update hepta_agent_bindings set version=$1, record_json=$2::jsonb, updated_at=$3
         where binding_id=$4 and version=$5",
    )
    .bind(binding.version as i64)
    .bind(record_json)
    .bind(binding.updated_at)
    .bind(binding_id)
    .bind(request.expected_binding_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "Agent binding changed concurrently",
        ));
    }
    let rotation_json = serde_json::to_value(&claim)
        .map_err(|error| ApiError::internal(format!("encode Agent key rotation: {error}")))?;
    let rotation_insert = sqlx::query(
        "insert into hepta_agent_binding_key_rotations (
            rotation_id, binding_id, nonce, expected_binding_version, record_json
         ) values ($1,$2,$3,$4,$5::jsonb)",
    )
    .bind(request.rotation_id)
    .bind(binding_id)
    .bind(&request.idempotency_key)
    .bind(request.expected_binding_version as i64)
    .bind(rotation_json)
    .execute(&mut *tx)
    .await;
    if let Err(error) = rotation_insert {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "agent_binding_rotation_reused",
                "Agent binding key rotation ID or nonce has already been accepted",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.agent_binding.key_rotated.v2",
        binding_id,
        binding.version,
        json!({
            "rotation_id": request.rotation_id,
            "binding_id": binding_id,
            "agent_id": binding.agent_id,
            "old_agent_key_id": claim.old_agent_key_id,
            "new_agent_key_id": binding.agent_key_id,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(binding_id),
        StatusCode::OK,
        &binding,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(binding)))
}

fn validate_agent_binding_rotation_scope(
    binding: &AgentBinding,
    claim: &AgentBindingKeyRotationClaimV2,
) -> Result<(), ApiError> {
    if binding.binding_id != claim.binding_id
        || binding.player_id != claim.player_id
        || binding.agent_id != claim.agent_id
        || binding.agent_key_id != claim.old_agent_key_id
        || binding.agent_public_key != claim.old_agent_public_key
        || binding.agent_public_key_hash != claim.old_agent_public_key_hash
        || binding.status != AgentBindingStatus::Active
    {
        return Err(ApiError::conflict(
            "agent_binding_rotation_scope_mismatch",
            "Agent binding identity or current key does not match the signed rotation",
        ));
    }
    if binding.version != claim.expected_binding_version {
        return Err(version_conflict(
            "Agent binding",
            claim.expected_binding_version,
            binding.version,
        ));
    }
    Ok(())
}

async fn create_team(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateResearchTeamRequest>,
) -> Result<(StatusCode, Json<ResearchTeam>), ApiError> {
    const OPERATION: &str = "create_research_team_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        "/v2/hepta/teams",
        &request.idempotency_key,
        &request_hash,
    )?;
    if !request
        .members
        .iter()
        .any(|member| member.player_id == assertion.player_id)
    {
        return Err(ApiError::forbidden(
            "user_not_on_team",
            "team creator must be included in the requested roster",
        ));
    }
    if state.pool.is_none() {
        {
            let memory = state.paper_raid.read().await;
            let player = memory.players.get(&assertion.player_id).ok_or_else(|| {
                ApiError::forbidden("human_player_not_registered", "human player does not exist")
            })?;
            assert_player_identity(&assertion, player)?;
            if let Some(replay) =
                memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
            {
                return Ok(replay);
            }
        }
        validate_new_research_team_request(&request)?;
        state
            .inspect(|league| {
                let challenge = league
                    .challenges
                    .get(&request.challenge_id)
                    .ok_or_else(|| {
                        ApiError::not_found(
                            "challenge_not_found",
                            "research challenge does not exist",
                        )
                    })?;
                if challenge.status != crate::ChallengeStatus::Open {
                    return Err(ApiError::conflict(
                        "challenge_not_open",
                        "Paper Raid teams can only form for an open challenge",
                    ));
                }
                Ok(())
            })
            .await?;
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let now = Utc::now();
        if memory
            .collaboration
            .team_proposals
            .contains_key(&request.team_id)
        {
            return Err(ApiError::conflict(
                "team_id_reserved_for_matchmaking",
                "team_id is reserved by a matchmaking proposal; use proposal materialization",
            ));
        }
        if memory.teams.contains_key(&request.team_id) {
            return Err(ApiError::conflict(
                "research_team_conflict",
                "team_id already exists",
            ));
        }
        let mut members = Vec::with_capacity(request.members.len());
        for requested in &request.members {
            let player = memory.players.get(&requested.player_id).ok_or_else(|| {
                ApiError::not_found("human_player_not_found", "team player does not exist")
            })?;
            if requested.player_id == assertion.player_id {
                assert_player_identity(&assertion, player)?;
            }
            if player.status != HumanPlayerStatus::Active {
                return Err(ApiError::conflict(
                    "human_player_not_active",
                    "team player is not active",
                ));
            }
            let binding = memory.bindings.get(&requested.binding_id).ok_or_else(|| {
                ApiError::not_found("agent_binding_not_found", "Agent binding does not exist")
            })?;
            if binding.status != AgentBindingStatus::Active
                || binding.player_id != requested.player_id
            {
                return Err(ApiError::conflict(
                    "agent_binding_player_mismatch",
                    "team Agent binding must be active and belong to its player",
                ));
            }
            members.push(TeamMember {
                participant_slot: requested.participant_slot,
                player_id: requested.player_id,
                binding_id: requested.binding_id,
                agent_id: binding.agent_id.clone(),
                role: requested.role.clone(),
                joined_at: now,
            });
        }
        members.sort_by_key(|member| member.participant_slot);
        let team = ResearchTeam {
            team_id: request.team_id,
            challenge_id: request.challenge_id,
            collaboration_compact_hash: request.collaboration_compact_hash.clone(),
            status: TeamStatus::Forming,
            roster_version: 1,
            members,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        memory.teams.insert(team.team_id, team.clone());
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.team.created.v2",
            team.team_id,
            team.version,
            json!({
                "team_id": team.team_id,
                "challenge_id": team.challenge_id,
                "roster_version": team.roster_version,
                "member_count": team.members.len(),
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &team,
        )?;
        return Ok((StatusCode::CREATED, Json(team)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    let replay_player_row =
        sqlx::query("select record_json from hepta_human_players where player_id=$1 for share")
            .bind(assertion.player_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::forbidden("human_player_not_registered", "human player does not exist")
            })?;
    let replay_player: HumanPlayer =
        decode_record(replay_player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &replay_player)?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    validate_new_research_team_request(&request)?;
    state
        .inspect(|league| {
            let challenge = league
                .challenges
                .get(&request.challenge_id)
                .ok_or_else(|| {
                    ApiError::not_found("challenge_not_found", "research challenge does not exist")
                })?;
            if challenge.status != crate::ChallengeStatus::Open {
                return Err(ApiError::conflict(
                    "challenge_not_open",
                    "Paper Raid teams can only form for an open challenge",
                ));
            }
            Ok(())
        })
        .await?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!(
            "hepta-paper-raid-matchmaking:{}",
            request.challenge_id
        ))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("hepta-paper-raid-team-id:{}", request.team_id))
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    if sqlx::query("select 1 from hepta_team_proposals where proposal_id=$1 limit 1")
        .bind(request.team_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .is_some()
    {
        return Err(ApiError::conflict(
            "team_id_reserved_for_matchmaking",
            "team_id is reserved by a matchmaking proposal; use proposal materialization",
        ));
    }
    let now = Utc::now();
    let mut members = Vec::with_capacity(request.members.len());
    for requested in &request.members {
        let row = sqlx::query(
            "select p.status as player_status, p.record_json as player_json,
                    b.player_id as binding_player_id, b.agent_id, b.status as binding_status
             from hepta_human_players p
             join hepta_agent_bindings b on b.binding_id = $2
             where p.player_id = $1
             for share of p, b",
        )
        .bind(requested.player_id)
        .bind(requested.binding_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::not_found(
                "team_member_not_found",
                "team player or Agent binding does not exist",
            )
        })?;
        let player_status: String = row.get("player_status");
        let binding_status: String = row.get("binding_status");
        let binding_player_id: Uuid = row.get("binding_player_id");
        if requested.player_id == assertion.player_id {
            let player: HumanPlayer = decode_record(row.get("player_json"), "human player")?;
            assert_player_identity(&assertion, &player)?;
        }
        if player_status != "active"
            || binding_status != "active"
            || binding_player_id != requested.player_id
        {
            return Err(ApiError::conflict(
                "agent_binding_player_mismatch",
                "team Agent binding must be active and belong to an active player",
            ));
        }
        members.push(TeamMember {
            participant_slot: requested.participant_slot,
            player_id: requested.player_id,
            binding_id: requested.binding_id,
            agent_id: row.get("agent_id"),
            role: requested.role.clone(),
            joined_at: now,
        });
    }
    members.sort_by_key(|member| member.participant_slot);
    let team = ResearchTeam {
        team_id: request.team_id,
        challenge_id: request.challenge_id,
        collaboration_compact_hash: request.collaboration_compact_hash.clone(),
        status: TeamStatus::Forming,
        roster_version: 1,
        members,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let record_json = serde_json::to_value(&team)
        .map_err(|error| ApiError::internal(format!("encode research team: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_research_teams (
            team_id, challenge_id, collaboration_compact_hash, status, roster_version, version,
            record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9)",
    )
    .bind(team.team_id)
    .bind(team.challenge_id)
    .bind(&team.collaboration_compact_hash)
    .bind(team.status.as_str())
    .bind(team.roster_version as i64)
    .bind(team.version as i64)
    .bind(record_json)
    .bind(team.created_at)
    .bind(team.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "research_team_conflict",
                "team_id already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    for member in &team.members {
        sqlx::query(
            "insert into hepta_research_team_members (
                team_id, participant_slot, player_id, binding_id, role, joined_at
             ) values ($1,$2,$3,$4,$5,$6)",
        )
        .bind(team.team_id)
        .bind(member.participant_slot as i32)
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
        "hepta.paper_raid.team.created.v2",
        team.team_id,
        team.version,
        json!({
            "team_id": team.team_id,
            "challenge_id": team.challenge_id,
            "roster_version": team.roster_version,
            "member_count": team.members.len(),
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(team.team_id),
        StatusCode::CREATED,
        &team,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(team)))
}

async fn get_team(
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ResearchTeam>, ApiError> {
    let path = format!("/v2/hepta/teams/{team_id}");
    let assertion = require_member_read_assertion(&headers, &state, "get_research_team_v2", &path)?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let team =
            memory.teams.get(&team_id).cloned().ok_or_else(|| {
                ApiError::not_found("research_team_not_found", "team does not exist")
            });
        let team = team?;
        assert_team_actor_memory(&memory, &team, &assertion)?;
        return Ok(Json(team));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let row = sqlx::query("select record_json from hepta_research_teams where team_id = $1")
        .bind(team_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
    assert_team_actor_postgres(&mut tx, team_id, &assertion).await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(decode_record(
        row.get("record_json"),
        "research team",
    )?))
}

fn build_team_acceptance_signing(
    team: &ResearchTeam,
    member: &TeamMember,
    player: &HumanPlayer,
    request: &AcceptResearchTeamMembershipRequest,
) -> Result<TeamMemberAcceptanceSigningV2, ApiError> {
    let accepted_at = Utc
        .timestamp_opt(request.accepted_at_unix, 0)
        .single()
        .ok_or_else(|| {
            ApiError::bad_request("invalid_accepted_at", "accepted_at_unix is out of range")
        })?;
    if request.expected_team_version != team.version
        || request.roster_version != team.roster_version
        || request.participant_slot != member.participant_slot
        || request.binding_id != member.binding_id
        || request.role != member.role
        || request.collaboration_compact_hash != team.collaboration_compact_hash
    {
        return Err(ApiError::conflict(
            "stale_team_proposal",
            "acceptance does not bind the current exact roster, role, Agent binding, and compact",
        ));
    }
    if team.status != TeamStatus::Forming {
        return Err(ApiError::conflict(
            "team_not_forming",
            "only a forming team proposal can be accepted",
        ));
    }
    let now = Utc::now();
    if accepted_at.timestamp() < team.created_at.timestamp()
        || accepted_at > now + chrono::Duration::minutes(5)
    {
        return Err(ApiError::bad_request(
            "invalid_accepted_at",
            "acceptance time predates the proposal or is too far in the future",
        ));
    }
    Ok(TeamMemberAcceptanceSigningV2 {
        schema: TEAM_MEMBER_ACCEPTANCE_V2.to_string(),
        acceptance_id: request.acceptance_id,
        team_id: team.team_id,
        challenge_id: team.challenge_id,
        roster_version: team.roster_version,
        participant_slot: member.participant_slot,
        player_id: member.player_id,
        binding_id: member.binding_id,
        agent_id: member.agent_id.clone(),
        role: member.role.clone(),
        collaboration_compact_hash: team.collaboration_compact_hash.clone(),
        signing_key_id: player.signing_key_id.clone(),
        signing_public_key: player.signing_public_key.clone(),
        signing_public_key_hash: player.signing_public_key_hash.clone(),
        accepted_at_unix: request.accepted_at_unix,
    })
}

fn acceptance_record(
    signing: TeamMemberAcceptanceSigningV2,
    signature: String,
) -> TeamMemberAcceptance {
    TeamMemberAcceptance {
        acceptance_id: signing.acceptance_id,
        team_id: signing.team_id,
        challenge_id: signing.challenge_id,
        roster_version: signing.roster_version,
        participant_slot: signing.participant_slot,
        player_id: signing.player_id,
        binding_id: signing.binding_id,
        agent_id: signing.agent_id,
        role: signing.role,
        collaboration_compact_hash: signing.collaboration_compact_hash,
        signing_key_id: signing.signing_key_id,
        signing_public_key: signing.signing_public_key,
        signing_public_key_hash: signing.signing_public_key_hash,
        accepted_at: Utc
            .timestamp_opt(signing.accepted_at_unix, 0)
            .single()
            .expect("validated acceptance timestamp"),
        superseded_at: None,
        signature,
    }
}

fn acceptance_signing_from_record(
    acceptance: &TeamMemberAcceptance,
) -> TeamMemberAcceptanceSigningV2 {
    TeamMemberAcceptanceSigningV2 {
        schema: TEAM_MEMBER_ACCEPTANCE_V2.to_string(),
        acceptance_id: acceptance.acceptance_id,
        team_id: acceptance.team_id,
        challenge_id: acceptance.challenge_id,
        roster_version: acceptance.roster_version,
        participant_slot: acceptance.participant_slot,
        player_id: acceptance.player_id,
        binding_id: acceptance.binding_id,
        agent_id: acceptance.agent_id.clone(),
        role: acceptance.role.clone(),
        collaboration_compact_hash: acceptance.collaboration_compact_hash.clone(),
        signing_key_id: acceptance.signing_key_id.clone(),
        signing_public_key: acceptance.signing_public_key.clone(),
        signing_public_key_hash: acceptance.signing_public_key_hash.clone(),
        accepted_at_unix: acceptance.accepted_at.timestamp(),
    }
}

fn ensure_team_fully_accepted(
    team: &ResearchTeam,
    acceptances: &[TeamMemberAcceptance],
    players: &HashMap<Uuid, HumanPlayer>,
    signing_keys: &HashMap<(Uuid, String), HumanSigningKey>,
) -> Result<(), ApiError> {
    if acceptances.len() != team.members.len() {
        return Err(ApiError::conflict(
            "team_acceptances_incomplete",
            "every proposed team member must sign the exact roster and collaboration compact",
        ));
    }
    for member in &team.members {
        let acceptance = acceptances
            .iter()
            .find(|acceptance| acceptance.player_id == member.player_id)
            .ok_or_else(|| {
                ApiError::conflict(
                    "team_acceptances_incomplete",
                    "a proposed team member has not accepted",
                )
            })?;
        if acceptance.team_id != team.team_id
            || acceptance.challenge_id != team.challenge_id
            || acceptance.roster_version != team.roster_version
            || acceptance.participant_slot != member.participant_slot
            || acceptance.binding_id != member.binding_id
            || acceptance.agent_id != member.agent_id
            || acceptance.role != member.role
            || acceptance.collaboration_compact_hash != team.collaboration_compact_hash
            || acceptance.superseded_at.is_some()
        {
            return Err(ApiError::conflict(
                "stale_team_acceptance",
                "stored member acceptance does not match the current team proposal",
            ));
        }
        let player = players
            .get(&member.player_id)
            .ok_or_else(|| ApiError::internal("team acceptance human player record is missing"))?;
        let key = signing_keys
            .get(&(member.player_id, acceptance.signing_key_id.clone()))
            .ok_or_else(|| {
                ApiError::conflict(
                    "team_acceptance_key_missing",
                    "acceptance signing key is absent from the human key history",
                )
            })?;
        if player.status != HumanPlayerStatus::Active
            || player.signing_key_id != acceptance.signing_key_id
            || player.signing_public_key != acceptance.signing_public_key
            || player.signing_public_key_hash != acceptance.signing_public_key_hash
            || key.status != HumanSigningKeyStatus::Active
            || key.retired_at.is_some()
            || key.revoked_at.is_some()
            || key.signing_public_key != acceptance.signing_public_key
            || key.signing_public_key_hash != acceptance.signing_public_key_hash
            // Signed contract timestamps are whole Unix seconds. Comparing
            // them to PostgreSQL/Rust sub-second timestamps would reject a
            // key used legitimately in the same second it was registered.
            || acceptance.accepted_at.timestamp() < key.registered_at.timestamp()
        {
            return Err(ApiError::conflict(
                "team_acceptance_key_not_current",
                "every member must re-accept with their current active, unrevoked human key",
            ));
        }
        verify_team_member_acceptance_signature(
            &acceptance_signing_from_record(acceptance),
            &acceptance.signature,
        )
        .map_err(|message| ApiError::forbidden("invalid_team_acceptance", message))?;
    }
    Ok(())
}

async fn list_team_acceptances(
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<TeamMemberAcceptance>>, ApiError> {
    let canonical_path = format!("/v2/hepta/teams/{team_id}/member-acceptances");
    let assertion = require_member_read_assertion(
        &headers,
        &state,
        "list_research_team_acceptances_v2",
        &canonical_path,
    )?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let team = memory
            .teams
            .get(&team_id)
            .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        let mut values: Vec<_> = memory
            .team_acceptances
            .values()
            .filter(|acceptance| {
                acceptance.team_id == team_id && acceptance.superseded_at.is_none()
            })
            .cloned()
            .collect();
        values.sort_by_key(|acceptance| acceptance.participant_slot);
        return Ok(Json(values));
    }
    let player_row = sqlx::query(
        "select p.record_json
         from hepta_research_team_members m
         join hepta_human_players p on p.player_id = m.player_id
         where m.team_id = $1 and m.player_id = $2",
    )
    .bind(team_id)
    .bind(assertion.player_id)
    .fetch_optional(state.pool.as_ref().expect("checked PostgreSQL pool"))
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::forbidden("user_not_on_team", "asserted player is not on the team"))?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let rows = sqlx::query(
        "select record_json from hepta_research_team_member_acceptances
         where team_id = $1 order by participant_slot",
    )
    .bind(team_id)
    .fetch_all(state.pool.as_ref().expect("checked PostgreSQL pool"))
    .await
    .map_err(ApiError::database)?;
    if rows.is_empty() {
        let exists = sqlx::query("select 1 from hepta_research_teams where team_id = $1")
            .bind(team_id)
            .fetch_optional(state.pool.as_ref().expect("checked PostgreSQL pool"))
            .await
            .map_err(ApiError::database)?
            .is_some();
        if !exists {
            return Err(ApiError::not_found(
                "research_team_not_found",
                "team does not exist",
            ));
        }
    }
    rows.into_iter()
        .map(|row| decode_record(row.get("record_json"), "team member acceptance"))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn accept_team_membership(
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<AcceptResearchTeamMembershipRequest>,
) -> Result<(StatusCode, Json<TeamMemberAcceptance>), ApiError> {
    const OPERATION: &str = "accept_research_team_membership_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contract_text_api("role", &request.role)?;
    validate_digest_v2(
        "collaboration_compact_hash",
        &request.collaboration_compact_hash,
    )?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/teams/{team_id}/member-acceptances");
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
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let team =
            memory.teams.get(&team_id).cloned().ok_or_else(|| {
                ApiError::not_found("research_team_not_found", "team does not exist")
            })?;
        let member = team
            .members
            .iter()
            .find(|member| member.player_id == assertion.player_id)
            .ok_or_else(|| {
                ApiError::forbidden("user_not_on_team", "asserted player is not on the team")
            })?;
        let player = memory
            .players
            .get(&assertion.player_id)
            .ok_or_else(|| ApiError::internal("team player record is missing"))?;
        assert_player_identity(&assertion, player)?;
        let signing_key = memory
            .human_signing_keys
            .get(&(player.player_id, player.signing_key_id.clone()))
            .ok_or_else(|| ApiError::internal("current human signing key record is missing"))?;
        if player.status != HumanPlayerStatus::Active
            || signing_key.status != HumanSigningKeyStatus::Active
            || signing_key.retired_at.is_some()
            || signing_key.revoked_at.is_some()
        {
            return Err(ApiError::forbidden(
                "human_signing_key_inactive",
                "team membership must be accepted with the current active human key",
            ));
        }
        let signing = build_team_acceptance_signing(&team, member, player, &request)?;
        verify_team_member_acceptance_signature(&signing, &request.signature)
            .map_err(|message| ApiError::forbidden("invalid_team_acceptance", message))?;
        if memory.team_acceptances.contains_key(&request.acceptance_id)
            || memory.team_acceptances.values().any(|acceptance| {
                acceptance.team_id == team_id
                    && acceptance.player_id == assertion.player_id
                    && acceptance.superseded_at.is_none()
            })
        {
            return Err(ApiError::conflict(
                "team_acceptance_conflict",
                "acceptance ID or team member acceptance already exists",
            ));
        }
        let response = acceptance_record(signing, request.signature.clone());
        memory
            .team_acceptances
            .insert(response.acceptance_id, response.clone());
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.team_member.accepted.v2",
            team.team_id,
            team.version,
            json!({
                "team_id": team.team_id,
                "roster_version": team.roster_version,
                "participant_slot": response.participant_slot,
                "player_id": response.player_id,
                "collaboration_compact_hash": team.collaboration_compact_hash,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &response,
        )?;
        return Ok((StatusCode::CREATED, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id = $1 for share")
            .bind(team_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    let member = team
        .members
        .iter()
        .find(|member| member.player_id == assertion.player_id)
        .ok_or_else(|| {
            ApiError::forbidden("user_not_on_team", "asserted player is not on the team")
        })?;
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id = $1 for share")
            .bind(assertion.player_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    let key_row = sqlx::query(
        "select record_json from hepta_human_signing_keys
         where player_id = $1 and signing_key_id = $2 for share",
    )
    .bind(player.player_id)
    .bind(&player.signing_key_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::internal("current human signing key record is missing"))?;
    let signing_key: HumanSigningKey =
        decode_record(key_row.get("record_json"), "human signing key")?;
    if player.status != HumanPlayerStatus::Active
        || signing_key.status != HumanSigningKeyStatus::Active
        || signing_key.retired_at.is_some()
        || signing_key.revoked_at.is_some()
    {
        return Err(ApiError::forbidden(
            "human_signing_key_inactive",
            "team membership must be accepted with the current active human key",
        ));
    }
    let signing = build_team_acceptance_signing(&team, member, &player, &request)?;
    verify_team_member_acceptance_signature(&signing, &request.signature)
        .map_err(|message| ApiError::forbidden("invalid_team_acceptance", message))?;
    let response = acceptance_record(signing, request.signature.clone());
    let record_json = serde_json::to_value(&response)
        .map_err(|error| ApiError::internal(format!("encode team acceptance: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_research_team_member_acceptances (
            acceptance_id, team_id, challenge_id, roster_version, participant_slot,
            player_id, binding_id, collaboration_compact_hash, signing_key_id,
            signing_public_key, signing_public_key_hash, signature, record_json,
            accepted_at, superseded_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::jsonb,$14,null)",
    )
    .bind(response.acceptance_id)
    .bind(response.team_id)
    .bind(response.challenge_id)
    .bind(response.roster_version as i64)
    .bind(response.participant_slot as i32)
    .bind(response.player_id)
    .bind(response.binding_id)
    .bind(&response.collaboration_compact_hash)
    .bind(&response.signing_key_id)
    .bind(&response.signing_public_key)
    .bind(&response.signing_public_key_hash)
    .bind(&response.signature)
    .bind(record_json)
    .bind(response.accepted_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "team_acceptance_conflict",
                "acceptance ID or team member acceptance already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.team_member.accepted.v2",
        team.team_id,
        team.version,
        json!({
            "team_id": team.team_id,
            "roster_version": team.roster_version,
            "participant_slot": response.participant_slot,
            "player_id": response.player_id,
            "collaboration_compact_hash": team.collaboration_compact_hash,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(team.team_id),
        StatusCode::CREATED,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn lock_team(
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<LockResearchTeamRequest>,
) -> Result<(StatusCode, Json<ResearchTeam>), ApiError> {
    const OPERATION: &str = "lock_research_team_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/teams/{team_id}/lock");
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
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let team_snapshot =
            memory.teams.get(&team_id).cloned().ok_or_else(|| {
                ApiError::not_found("research_team_not_found", "team does not exist")
            })?;
        assert_team_actor_memory(&memory, &team_snapshot, &assertion)?;
        let acceptances: Vec<_> = memory
            .team_acceptances
            .values()
            .filter(|acceptance| {
                acceptance.team_id == team_id && acceptance.superseded_at.is_none()
            })
            .cloned()
            .collect();
        ensure_team_fully_accepted(
            &team_snapshot,
            &acceptances,
            &memory.players,
            &memory.human_signing_keys,
        )?;
        let team = memory.teams.get_mut(&team_id).expect("team exists");
        if team.version != request.expected_version {
            return Err(version_conflict(
                "research team",
                request.expected_version,
                team.version,
            ));
        }
        if team.status != TeamStatus::Forming || !(3..=5).contains(&team.members.len()) {
            return Err(ApiError::conflict(
                "team_not_lockable",
                "only a forming 3-5 member team can be locked",
            ));
        }
        team.status = TeamStatus::Locked;
        team.version += 1;
        team.updated_at = Utc::now();
        let response = team.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.team.locked.v2",
            response.team_id,
            response.version,
            json!({
                "team_id": response.team_id,
                "roster_version": response.roster_version,
                "member_count": response.members.len(),
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let row = sqlx::query(
        "select version, record_json from hepta_research_teams where team_id = $1 for update",
    )
    .bind(team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
    assert_team_actor_postgres(&mut tx, team_id, &assertion).await?;
    let current_version: i64 = row.get("version");
    let mut team: ResearchTeam = decode_record(row.get("record_json"), "research team")?;
    if u64::try_from(current_version).ok() != Some(request.expected_version) {
        return Err(version_conflict(
            "research team",
            request.expected_version,
            team.version,
        ));
    }
    if team.status != TeamStatus::Forming || !(3..=5).contains(&team.members.len()) {
        return Err(ApiError::conflict(
            "team_not_lockable",
            "only a forming 3-5 member team can be locked",
        ));
    }
    let acceptance_rows = sqlx::query(
        "select record_json from hepta_research_team_member_acceptances
         where team_id = $1 and superseded_at is null
         order by participant_slot for share",
    )
    .bind(team_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let acceptances = acceptance_rows
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "team member acceptance"))
        .collect::<Result<Vec<TeamMemberAcceptance>, _>>()?;
    let mut players = HashMap::new();
    let mut signing_keys = HashMap::new();
    for member in &team.members {
        let player_row = sqlx::query(
            "select record_json from hepta_human_players where player_id = $1 for share",
        )
        .bind(member.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
        let key_row = sqlx::query(
            "select record_json from hepta_human_signing_keys
             where player_id = $1 and signing_key_id = $2 for share",
        )
        .bind(member.player_id)
        .bind(&player.signing_key_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::internal("current human signing key record is missing"))?;
        let key: HumanSigningKey = decode_record(key_row.get("record_json"), "human signing key")?;
        signing_keys.insert((member.player_id, key.signing_key_id.clone()), key);
        players.insert(member.player_id, player);
    }
    ensure_team_fully_accepted(&team, &acceptances, &players, &signing_keys)?;
    team.status = TeamStatus::Locked;
    team.version += 1;
    team.updated_at = Utc::now();
    let record_json = serde_json::to_value(&team)
        .map_err(|error| ApiError::internal(format!("encode research team: {error}")))?;
    let updated = sqlx::query(
        "update hepta_research_teams
         set status = $1, version = $2, record_json = $3::jsonb, updated_at = $4
         where team_id = $5 and version = $6",
    )
    .bind(team.status.as_str())
    .bind(team.version as i64)
    .bind(record_json)
    .bind(team.updated_at)
    .bind(team.team_id)
    .bind(request.expected_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "research team changed concurrently",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.team.locked.v2",
        team.team_id,
        team.version,
        json!({
            "team_id": team.team_id,
            "roster_version": team.roster_version,
            "member_count": team.members.len(),
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(team.team_id),
        StatusCode::OK,
        &team,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(team)))
}

fn version_conflict(kind: &str, expected: u64, actual: u64) -> ApiError {
    ApiError::conflict(
        "aggregate_version_conflict",
        format!("{kind} expected version {expected}, current version is {actual}"),
    )
}

async fn ensure_paper_finality_v2_source_unsealed_memory(
    state: &AppState,
    paper_id: Uuid,
) -> Result<(), ApiError> {
    let finality = state.paper_chain_finality.read().await;
    crate::paper_chain_finality_v2::ensure_paper_finality_v2_source_unsealed_memory(
        &finality, paper_id,
    )
}

type ChallengeRulesetSnapshotOutcome = (
    PaperChallengeRulesetSnapshotV1,
    String,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
);

fn snapshot_challenge_ruleset(
    challenge: &ResearchChallenge,
    activation: Option<&crate::challenge_pack_activation::ChallengePackActivationRecordV1>,
    started_at: DateTime<Utc>,
) -> Result<ChallengeRulesetSnapshotOutcome, ApiError> {
    let challenge_snapshot_hash = crate::challenge_snapshot_hash(challenge)?;
    let material_authority = activation
        .map(|activation| {
            crate::challenge_pack_activation::freeze_challenge_material_authority(
                challenge,
                activation,
                &challenge_snapshot_hash,
            )
        })
        .transpose()?;
    let (enforcement, ruleset, deadline_at, grace_expires_at) =
        if let Some(ruleset) = &challenge.ruleset {
            ruleset.validate().map_err(|message| {
                ApiError::internal(format!("stored challenge ruleset is invalid: {message}"))
            })?;
            let computed_hash = ruleset.canonical_hash().map_err(|message| {
                ApiError::internal(format!("hash stored challenge ruleset: {message}"))
            })?;
            if challenge.ruleset_hash != computed_hash {
                return Err(ApiError::internal(
                    "stored challenge ruleset hash does not match its canonical typed ruleset",
                ));
            }
            let deadline_at = started_at
                .checked_add_signed(Duration::seconds(i64::from(ruleset.duration_seconds)))
                .ok_or_else(|| ApiError::internal("challenge deadline overflow"))?;
            let grace_expires_at = deadline_at
                .checked_add_signed(Duration::seconds(i64::from(ruleset.grace_seconds)))
                .ok_or_else(|| ApiError::internal("challenge grace deadline overflow"))?;
            (
                ChallengeRulesetEnforcementV1::AuthoritativeV1,
                Some(ruleset.clone()),
                Some(deadline_at),
                Some(grace_expires_at),
            )
        } else {
            (
                ChallengeRulesetEnforcementV1::LegacyUnranked,
                None,
                None,
                None,
            )
        };
    let snapshot = PaperChallengeRulesetSnapshotV1 {
        schema: CHALLENGE_RULESET_SNAPSHOT_V1.to_string(),
        challenge_snapshot_hash,
        ruleset_version: challenge.ruleset_version.clone(),
        ruleset_hash: challenge.ruleset_hash.clone(),
        enforcement,
        ruleset,
        material_authority,
    };
    let snapshot_hash = snapshot.canonical_hash().map_err(|message| {
        ApiError::internal(format!("hash paper challenge ruleset snapshot: {message}"))
    })?;
    Ok((snapshot, snapshot_hash, deadline_at, grace_expires_at))
}

pub(super) fn ensure_paper_gameplay_active(
    paper: &PaperProject,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if paper.outcome != PaperChallengeOutcomeV1::InProgress {
        return Err(ApiError::conflict(
            "paper_challenge_terminal",
            format!(
                "paper challenge outcome {} is terminal",
                paper.outcome.as_str()
            ),
        ));
    }
    let deadline = if paper.active_rework_id.is_some()
        && paper.active_rework_cycle.is_some()
        && paper.rework_expires_at.is_some()
    {
        paper.rework_expires_at
    } else if paper.active_rework_id.is_none()
        && paper.active_rework_cycle.is_none()
        && paper.rework_expires_at.is_none()
    {
        paper.grace_expires_at
    } else {
        return Err(ApiError::internal(
            "Paper rework lease fields are incomplete",
        ));
    };
    if deadline.is_some_and(|expires_at| now >= expires_at) {
        return Err(ApiError::conflict(
            if paper.active_rework_id.is_some() {
                "paper_rework_window_elapsed"
            } else {
                "paper_challenge_deadline_elapsed"
            },
            if paper.active_rework_id.is_some() {
                "Paper rework lease elapsed; record the expired terminal outcome"
            } else {
                "paper challenge grace deadline elapsed; record the expired terminal outcome"
            },
        ));
    }
    Ok(())
}

async fn snapshot_open_challenge_ruleset(
    state: &AppState,
    challenge_id: Uuid,
    started_at: DateTime<Utc>,
) -> Result<ChallengeRulesetSnapshotOutcome, ApiError> {
    let (challenge, activation) = state
        .inspect(|league| {
            let challenge = league
                .challenges
                .get(&challenge_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::not_found(
                        "challenge_not_found",
                        "paper project challenge does not exist",
                    )
                })?;
            let activation = league
                .challenge_pack_activations
                .get(&challenge_id)
                .cloned();
            Ok((challenge, activation))
        })
        .await?;
    if challenge.status != crate::ChallengeStatus::Open {
        return Err(ApiError::conflict(
            "challenge_not_open",
            "paper projects can only snapshot an open challenge",
        ));
    }
    snapshot_challenge_ruleset(&challenge, activation.as_ref(), started_at)
}

async fn create_paper(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperProjectRequest>,
) -> Result<(StatusCode, Json<PaperProject>), ApiError> {
    const OPERATION: &str = "create_paper_project_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contract_text_api("title", &request.title)?;
    validate_contract_text_api("target_format", &request.target_format)?;
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        "/v2/hepta/papers",
        &request.idempotency_key,
        &request_hash,
    )?;
    if state.pool.is_none() {
        let now = Utc::now();
        let challenge_id = {
            let memory = state.paper_raid.read().await;
            if let Some(replay) =
                memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
            {
                return Ok(replay);
            }
            memory
                .teams
                .get(&request.team_id)
                .map(|team| team.challenge_id)
                .ok_or_else(|| {
                    ApiError::not_found("research_team_not_found", "team does not exist")
                })?
        };
        let (ruleset_snapshot, ruleset_snapshot_hash, deadline_at, grace_expires_at) =
            snapshot_open_challenge_ruleset(&state, challenge_id, now).await?;
        let role_resources = role_resource_state_from_snapshot(&ruleset_snapshot, now)?;
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let team = memory
            .teams
            .get(&request.team_id)
            .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        if team.status != TeamStatus::Locked {
            return Err(ApiError::conflict(
                "research_team_not_locked",
                "paper project requires a locked research team",
            ));
        }
        if memory.papers.contains_key(&request.paper_project_id)
            || memory
                .papers
                .values()
                .any(|paper| paper.team_id == request.team_id)
        {
            return Err(ApiError::conflict(
                "paper_project_conflict",
                "paper_project_id or team paper project already exists",
            ));
        }
        let paper = PaperProject {
            paper_project_id: request.paper_project_id,
            team_id: team.team_id,
            challenge_id: team.challenge_id,
            title: request.title.clone(),
            target_format: request.target_format.clone(),
            phase: PaperPhase::Forming,
            challenge_ruleset_snapshot: Some(ruleset_snapshot),
            challenge_ruleset_snapshot_hash: Some(ruleset_snapshot_hash),
            deadline_at,
            grace_expires_at,
            active_rework_id: None,
            active_rework_cycle: None,
            rework_expires_at: None,
            outcome: PaperChallengeOutcomeV1::InProgress,
            outcome_reason: None,
            terminal_at: None,
            role_resources,
            current_revision_id: None,
            release_candidate_revision_id: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        memory.papers.insert(paper.paper_project_id, paper.clone());
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.paper_project.created.v2",
            paper.paper_project_id,
            paper.version,
            json!({
                "paper_project_id": paper.paper_project_id,
                "team_id": paper.team_id,
                "challenge_id": paper.challenge_id,
                "phase": paper.phase,
                "challenge_ruleset_snapshot_hash": paper.challenge_ruleset_snapshot_hash.as_deref(),
                "ruleset_enforcement": paper.challenge_ruleset_snapshot.as_ref().map(|snapshot| snapshot.enforcement),
                "deadline_at": paper.deadline_at,
                "grace_expires_at": paper.grace_expires_at,
                "outcome": paper.outcome,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &paper,
        )?;
        return Ok((StatusCode::CREATED, Json(paper)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let now = collaboration_v3::postgres_transaction_now(&mut tx).await?;
    let team_row = sqlx::query(
        "select challenge_id, status, record_json from hepta_research_teams
         where team_id = $1 for share",
    )
    .bind(request.team_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("research_team_not_found", "team does not exist"))?;
    let team_status: String = team_row.get("status");
    assert_team_actor_postgres(&mut tx, request.team_id, &assertion).await?;
    if team_status != "locked" {
        return Err(ApiError::conflict(
            "research_team_not_locked",
            "paper project requires a locked research team",
        ));
    }
    let challenge_id: Uuid = team_row.get("challenge_id");
    let (ruleset_snapshot, ruleset_snapshot_hash, deadline_at, grace_expires_at) =
        snapshot_open_challenge_ruleset(&state, challenge_id, now).await?;
    let role_resources = role_resource_state_from_snapshot(&ruleset_snapshot, now)?;
    let paper = PaperProject {
        paper_project_id: request.paper_project_id,
        team_id: request.team_id,
        challenge_id,
        title: request.title.clone(),
        target_format: request.target_format.clone(),
        phase: PaperPhase::Forming,
        challenge_ruleset_snapshot: Some(ruleset_snapshot),
        challenge_ruleset_snapshot_hash: Some(ruleset_snapshot_hash),
        deadline_at,
        grace_expires_at,
        active_rework_id: None,
        active_rework_cycle: None,
        rework_expires_at: None,
        outcome: PaperChallengeOutcomeV1::InProgress,
        outcome_reason: None,
        terminal_at: None,
        role_resources,
        current_revision_id: None,
        release_candidate_revision_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let record_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_paper_projects (
            paper_project_id, team_id, challenge_id, phase,
            challenge_ruleset_snapshot_hash, deadline_at, grace_expires_at,
            outcome, outcome_reason, terminal_at, version,
            record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12::jsonb,$13,$14)",
    )
    .bind(paper.paper_project_id)
    .bind(paper.team_id)
    .bind(paper.challenge_id)
    .bind(paper.phase.as_str())
    .bind(&paper.challenge_ruleset_snapshot_hash)
    .bind(paper.deadline_at)
    .bind(paper.grace_expires_at)
    .bind(paper.outcome.as_str())
    .bind(&paper.outcome_reason)
    .bind(paper.terminal_at)
    .bind(paper.version as i64)
    .bind(record_json)
    .bind(paper.created_at)
    .bind(paper.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_project_conflict",
                "paper_project_id or team paper project already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.paper_project.created.v2",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "team_id": paper.team_id,
            "challenge_id": paper.challenge_id,
            "phase": paper.phase,
            "challenge_ruleset_snapshot_hash": paper.challenge_ruleset_snapshot_hash.as_deref(),
            "ruleset_enforcement": paper.challenge_ruleset_snapshot.as_ref().map(|snapshot| snapshot.enforcement),
            "deadline_at": paper.deadline_at,
            "grace_expires_at": paper.grace_expires_at,
            "outcome": paper.outcome,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper.paper_project_id),
        StatusCode::CREATED,
        &paper,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(paper)))
}

async fn get_paper(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PaperProject>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}");
    let assertion = require_member_read_assertion(&headers, &state, "get_paper_project_v2", &path)?;
    collaboration_v3::ensure_automatic_challenge_expiry_materialized(
        &state,
        paper_id,
        &assertion,
        Utc::now(),
    )
    .await?;
    if state.pool.is_none() {
        let memory = state.paper_raid.read().await;
        let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        collaboration_v3::validate_paper_role_resources(&paper)?;
        return Ok(Json(paper));
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let row = sqlx::query(
        "select team_id, record_json from hepta_paper_projects where paper_project_id = $1",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let team_id: Uuid = row.get("team_id");
    assert_team_actor_postgres(&mut tx, team_id, &assertion).await?;
    let paper = decode_record(row.get("record_json"), "paper project")?;
    collaboration_v3::validate_paper_role_resources(&paper)?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(paper))
}

fn validate_requested_terminal_outcome(
    paper: &PaperProject,
    requested: PaperChallengeOutcomeV1,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if paper.outcome != PaperChallengeOutcomeV1::InProgress {
        return Err(ApiError::conflict(
            "paper_challenge_terminal",
            "paper challenge already has a terminal outcome",
        ));
    }
    let active_rework = paper.active_rework_id.is_some()
        && paper.active_rework_cycle.is_some()
        && paper.rework_expires_at.is_some();
    let expiry_boundary = if active_rework {
        paper.rework_expires_at
    } else if paper.active_rework_id.is_none()
        && paper.active_rework_cycle.is_none()
        && paper.rework_expires_at.is_none()
    {
        paper.grace_expires_at
    } else {
        return Err(ApiError::internal(
            "Paper rework lease fields are incomplete",
        ));
    };
    let expiry_elapsed = expiry_boundary.is_some_and(|expires_at| now >= expires_at);
    match requested {
        PaperChallengeOutcomeV1::Failed | PaperChallengeOutcomeV1::Abandoned => {
            if expiry_elapsed {
                return Err(ApiError::conflict(
                    if active_rework {
                        "paper_rework_window_elapsed"
                    } else {
                        "paper_challenge_deadline_elapsed"
                    },
                    "an authoritative Paper at or after its active deadline must record expired",
                ));
            }
            Ok(())
        }
        PaperChallengeOutcomeV1::Expired => {
            if expiry_boundary.is_none() {
                return Err(ApiError::conflict(
                    "legacy_challenge_has_no_deadline",
                    "legacy-unranked paper projects have no authoritative deadline to expire",
                ));
            }
            if !expiry_elapsed {
                return Err(ApiError::conflict(
                    "paper_challenge_not_expired",
                    "expired outcome is only valid at or after the immutable challenge or rework deadline",
                ));
            }
            Ok(())
        }
        PaperChallengeOutcomeV1::InProgress | PaperChallengeOutcomeV1::SubmissionReady => {
            Err(ApiError::bad_request(
                "invalid_paper_challenge_outcome",
                "outcome command accepts only failed, expired, or abandoned",
            ))
        }
    }
}

async fn transition_paper_outcome(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<TransitionPaperOutcomeRequestV1>,
) -> Result<(StatusCode, Json<PaperProject>), ApiError> {
    const OPERATION: &str = "transition_paper_challenge_outcome_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contract_text_api("reason_code", &request.reason_code)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/outcome");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();

    // Canonical automatic expiry is authorized and committed before the
    // Captain's manual command can acquire the aggregate mutation lock. At or
    // after the grace boundary this makes the immutable automatic reason and
    // grace-based terminal timestamp win every race; the manual request then
    // observes a terminal/version conflict instead of supplying its own fact.
    collaboration_v3::ensure_automatic_challenge_expiry_materialized(
        &state, paper_id, &assertion, now,
    )
    .await?;

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        let snapshot = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        if snapshot.version != request.expected_version {
            return Err(version_conflict(
                "paper project",
                request.expected_version,
                snapshot.version,
            ));
        }
        let team = memory
            .teams
            .get(&snapshot.team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        require_author_role(
            team,
            assertion.player_id,
            "captain",
            "paper terminal outcome",
        )?;
        validate_requested_terminal_outcome(&snapshot, request.outcome, now)?;
        let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
        paper.outcome = request.outcome;
        paper.outcome_reason = Some(request.reason_code.clone());
        paper.terminal_at = Some(now);
        paper.active_rework_id = None;
        paper.active_rework_cycle = None;
        paper.rework_expires_at = None;
        paper.version = paper
            .version
            .checked_add(1)
            .ok_or_else(|| ApiError::internal("paper version overflow"))?;
        paper.updated_at = now;
        let response = paper.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.challenge_outcome.terminal.v1",
            response.paper_project_id,
            response.version,
            json!({
                "paper_project_id": response.paper_project_id,
                "outcome": response.outcome,
                "reason_code": response.outcome_reason,
                "terminal_at": response.terminal_at,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let now = collaboration_v3::postgres_transaction_now(&mut tx).await?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let row = sqlx::query(
        "select version,record_json from hepta_paper_projects where paper_project_id=$1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(row.get("record_json"), "paper project")?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if actual_version != request.expected_version {
        return Err(version_conflict(
            "paper project",
            request.expected_version,
            actual_version,
        ));
    }
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    assert_team_actor_postgres(&mut tx, team.team_id, &assertion).await?;
    require_author_role(
        &team,
        assertion.player_id,
        "captain",
        "paper terminal outcome",
    )?;
    validate_requested_terminal_outcome(&paper, request.outcome, now)?;
    paper.outcome = request.outcome;
    paper.outcome_reason = Some(request.reason_code.clone());
    paper.terminal_at = Some(now);
    paper.active_rework_id = None;
    paper.active_rework_cycle = None;
    paper.rework_expires_at = None;
    paper.version = paper
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("paper version overflow"))?;
    paper.updated_at = now;
    let record_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set outcome=$1,outcome_reason=$2,terminal_at=$3,
             active_rework_id=null,active_rework_cycle=null,rework_expires_at=null,
             version=$4,record_json=$5::jsonb,updated_at=$6
         where paper_project_id=$7 and version=$8",
    )
    .bind(paper.outcome.as_str())
    .bind(&paper.outcome_reason)
    .bind(paper.terminal_at)
    .bind(paper.version as i64)
    .bind(record_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_version as i64)
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
        "hepta.paper_raid.challenge_outcome.terminal.v1",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "outcome": paper.outcome,
            "reason_code": paper.outcome_reason,
            "terminal_at": paper.terminal_at,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper.paper_project_id),
        StatusCode::OK,
        &paper,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(paper)))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AuthorPhaseGateFacts {
    work_item: bool,
    work_item_count: u32,
    accepted_work_item_count: u32,
    all_work_items_terminal: bool,
    paper_revision: bool,
    paper_revision_count: u32,
    paper_revision_covers_section_merges: bool,
    collaboration: collaboration_v3::CollaborationPhaseGateFacts,
    collaboration_counts: collaboration_v3::CollaborationPhaseGateCounts,
}

fn authoritative_paper_ruleset(
    paper: &PaperProject,
) -> Result<Option<&crate::ChallengeRulesetV1>, ApiError> {
    let Some(snapshot) = &paper.challenge_ruleset_snapshot else {
        // Records created before ChallengeRuleset V1 remain readable and use
        // the conservative legacy gates below. They are never ranked or
        // economically eligible.
        return Ok(None);
    };
    let expected_hash = snapshot.canonical_hash().map_err(|message| {
        ApiError::internal(format!("hash paper challenge ruleset snapshot: {message}"))
    })?;
    if paper.challenge_ruleset_snapshot_hash.as_deref() != Some(expected_hash.as_str()) {
        return Err(ApiError::internal(
            "paper challenge ruleset snapshot hash does not match the immutable snapshot",
        ));
    }
    match snapshot.enforcement {
        ChallengeRulesetEnforcementV1::LegacyUnranked => {
            if snapshot.ruleset.is_some() {
                return Err(ApiError::internal(
                    "legacy-unranked challenge snapshot unexpectedly contains a typed ruleset",
                ));
            }
            Ok(None)
        }
        ChallengeRulesetEnforcementV1::AuthoritativeV1 => {
            let ruleset = snapshot.ruleset.as_ref().ok_or_else(|| {
                ApiError::internal("authoritative challenge snapshot is missing its typed ruleset")
            })?;
            ruleset.validate().map_err(|message| {
                ApiError::internal(format!("paper challenge ruleset is invalid: {message}"))
            })?;
            let ruleset_hash = ruleset.canonical_hash().map_err(|message| {
                ApiError::internal(format!("hash paper challenge ruleset: {message}"))
            })?;
            if snapshot.ruleset_hash != ruleset_hash {
                return Err(ApiError::internal(
                    "paper challenge ruleset hash does not match the typed ruleset",
                ));
            }
            Ok(Some(ruleset))
        }
    }
}

fn paper_challenge_binding_hashes(
    paper: &PaperProject,
    current_challenge: &ResearchChallenge,
) -> Result<(String, String), ApiError> {
    if let Some(snapshot) = &paper.challenge_ruleset_snapshot {
        // This verifies the full snapshot, its typed ruleset, and both hashes
        // before any downstream signed claim binds them.
        let _ = authoritative_paper_ruleset(paper)?;
        return Ok((
            crate::canonical_digest("ruleset_hash", &snapshot.ruleset_hash)?,
            crate::canonical_digest("challenge_snapshot_hash", &snapshot.challenge_snapshot_hash)?,
        ));
    }
    // Pre-V1 Paper records preserve their historical signed bytes.
    Ok((
        crate::canonical_digest("ruleset_hash", &current_challenge.ruleset_hash)?,
        crate::challenge_snapshot_hash(current_challenge)?,
    ))
}

fn forward_transition(
    current: PaperPhase,
    next: PaperPhase,
) -> Option<ChallengeForwardTransitionV1> {
    match (current, next) {
        (PaperPhase::Preregistering, PaperPhase::Researching) => {
            Some(ChallengeForwardTransitionV1::PreregisteringToResearching)
        }
        (PaperPhase::Researching, PaperPhase::Experimenting) => {
            Some(ChallengeForwardTransitionV1::ResearchingToExperimenting)
        }
        (PaperPhase::Experimenting, PaperPhase::Drafting) => {
            Some(ChallengeForwardTransitionV1::ExperimentingToDrafting)
        }
        (PaperPhase::Drafting, PaperPhase::IntegrityReview) => {
            Some(ChallengeForwardTransitionV1::DraftingToIntegrityReview)
        }
        (PaperPhase::IntegrityReview, PaperPhase::Reproducing) => {
            Some(ChallengeForwardTransitionV1::IntegrityReviewToReproducing)
        }
        (PaperPhase::Reproducing, PaperPhase::AuthorApproval) => {
            Some(ChallengeForwardTransitionV1::ReproducingToAuthorApproval)
        }
        _ => None,
    }
}

fn observed_requirement(
    requirement: ChallengeRequirementKindV1,
    facts: AuthorPhaseGateFacts,
    release_candidate: bool,
    all_author_consents: bool,
) -> u32 {
    match requirement {
        ChallengeRequirementKindV1::WorkItems => facts.work_item_count,
        ChallengeRequirementKindV1::AcceptedWorkItems => facts.accepted_work_item_count,
        ChallengeRequirementKindV1::ArtifactManifests => {
            facts.collaboration_counts.artifact_manifests
        }
        ChallengeRequirementKindV1::EvidenceCards => facts.collaboration_counts.evidence_cards,
        ChallengeRequirementKindV1::Citations => facts.collaboration_counts.citations,
        ChallengeRequirementKindV1::ExperimentPlans => facts.collaboration_counts.experiment_plans,
        ChallengeRequirementKindV1::RetainedRuns => facts.collaboration_counts.retained_runs,
        ChallengeRequirementKindV1::SuccessfulRuns => facts.collaboration_counts.successful_runs,
        ChallengeRequirementKindV1::RetainedFailedRuns => {
            facts.collaboration_counts.retained_failed_runs
        }
        ChallengeRequirementKindV1::Claims => facts.collaboration_counts.claims,
        ChallengeRequirementKindV1::SectionRevisions => {
            facts.collaboration_counts.section_revisions
        }
        ChallengeRequirementKindV1::ApprovingSectionReviews => {
            facts.collaboration_counts.approving_section_reviews
        }
        ChallengeRequirementKindV1::SectionMerges => facts.collaboration_counts.section_merges,
        ChallengeRequirementKindV1::PaperRevisions => facts.paper_revision_count,
        ChallengeRequirementKindV1::AllWorkItemsTerminal => {
            u32::from(facts.all_work_items_terminal)
        }
        ChallengeRequirementKindV1::PaperRevisionCoversSectionMerges => {
            u32::from(facts.paper_revision_covers_section_merges)
        }
        ChallengeRequirementKindV1::ReleaseCandidate => u32::from(release_candidate),
        ChallengeRequirementKindV1::AllAuthorConsents => u32::from(all_author_consents),
    }
}

fn typed_requirement_blockers(
    requirements: &[ChallengeMinimumV1],
    facts: AuthorPhaseGateFacts,
    release_candidate: bool,
    all_author_consents: bool,
) -> Vec<String> {
    requirements
        .iter()
        .filter_map(|requirement| {
            let observed = observed_requirement(
                requirement.kind,
                facts,
                release_candidate,
                all_author_consents,
            );
            (observed < u32::from(requirement.minimum)).then(|| {
                format!(
                    "minimum_{}_required:{}:{}",
                    requirement.kind.code(),
                    requirement.minimum,
                    observed
                )
            })
        })
        .collect()
}

fn author_phase_gate_blockers(
    paper: &PaperProject,
    next_phase: PaperPhase,
    facts: AuthorPhaseGateFacts,
) -> Result<Vec<String>, ApiError> {
    if let (Some(ruleset), Some(transition)) = (
        authoritative_paper_ruleset(paper)?,
        forward_transition(paper.phase, next_phase),
    ) {
        return Ok(typed_requirement_blockers(
            ruleset.requirements_for(transition),
            facts,
            false,
            false,
        ));
    }
    let mut blockers = Vec::new();
    let mut require = |condition: bool, code: &'static str| {
        if !condition {
            blockers.push(code.to_string());
        }
    };
    match (paper.phase, next_phase) {
        (PaperPhase::Preregistering, PaperPhase::Researching) => {
            require(facts.work_item, "work_item_required");
            require(
                facts.collaboration.artifact_manifest,
                "artifact_manifest_required",
            );
            require(
                facts.collaboration.experiment_plan,
                "experiment_plan_required",
            );
        }
        (PaperPhase::Researching, PaperPhase::Experimenting) => {
            require(facts.collaboration.evidence_card, "evidence_card_required");
            require(facts.collaboration.citation, "citation_required");
            require(facts.collaboration.claim, "claim_required");
        }
        (PaperPhase::Experimenting, PaperPhase::Drafting) => {
            require(facts.collaboration.retained_run, "retained_run_required");
            require(
                facts.collaboration.artifact_manifest,
                "artifact_manifest_required",
            );
        }
        (PaperPhase::Drafting, PaperPhase::IntegrityReview) => {
            require(
                facts.all_work_items_terminal,
                "work_items_must_be_accepted_or_cancelled",
            );
            require(
                facts.collaboration.section_revision,
                "section_revision_required",
            );
            require(facts.paper_revision, "paper_revision_required");
        }
        (PaperPhase::IntegrityReview, PaperPhase::Reproducing) => {
            require(
                facts.collaboration.approving_section_review,
                "approving_section_review_required",
            );
            require(facts.collaboration.section_merge, "section_merge_required");
        }
        (PaperPhase::Reproducing, PaperPhase::AuthorApproval) => {
            require(facts.paper_revision, "paper_revision_required");
            if facts.paper_revision {
                require(
                    facts.paper_revision_covers_section_merges,
                    "paper_revision_section_lineage_required",
                );
            }
        }
        _ => {}
    }
    Ok(blockers)
}

fn author_phase_gate_facts_memory(
    memory: &PaperRaidMemory,
    paper: &PaperProject,
) -> AuthorPhaseGateFacts {
    let work_items = memory
        .work_items
        .values()
        .filter(|item| item.paper_project_id == paper.paper_project_id)
        .collect::<Vec<_>>();
    AuthorPhaseGateFacts {
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
        paper_revision: paper.current_revision_id.is_some()
            && memory
                .revisions
                .values()
                .any(|revision| revision.paper_project_id == paper.paper_project_id),
        paper_revision_count: memory
            .revisions
            .values()
            .filter(|revision| revision.paper_project_id == paper.paper_project_id)
            .count() as u32,
        paper_revision_covers_section_merges:
            collaboration_v3::paper_revision_covers_section_merges_memory(memory, paper),
        collaboration: collaboration_v3::collaboration_phase_gate_facts_memory(
            &memory.collaboration,
            paper.paper_project_id,
        ),
        collaboration_counts: collaboration_v3::collaboration_phase_gate_counts_memory(
            &memory.collaboration,
            paper.paper_project_id,
        ),
    }
}

async fn author_phase_gate_facts_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper: &PaperProject,
) -> Result<AuthorPhaseGateFacts, ApiError> {
    let row = sqlx::query(
        "select
           exists(select 1 from hepta_paper_work_items where paper_project_id=$1 and status<>'cancelled') as work_item,
           (select count(*) from hepta_paper_work_items where paper_project_id=$1 and status<>'cancelled') as work_item_count,
           (select count(*) from hepta_paper_work_items where paper_project_id=$1 and status='accepted') as accepted_work_item_count,
           exists(select 1 from hepta_paper_work_items where paper_project_id=$1)
             and not exists(
               select 1 from hepta_paper_work_items
               where paper_project_id=$1 and status not in ('accepted','cancelled')
             ) as all_work_items_terminal,
           exists(select 1 from hepta_paper_revisions where paper_project_id=$1) as paper_revision,
           (select count(*) from hepta_paper_revisions where paper_project_id=$1) as paper_revision_count,
           exists(select 1 from hepta_artifact_manifests where paper_project_id=$1) as artifact_manifest,
           (select count(*) from hepta_artifact_manifests where paper_project_id=$1) as artifact_manifest_count,
           exists(select 1 from hepta_evidence_cards where paper_project_id=$1) as evidence_card,
           (select count(*) from hepta_evidence_cards where paper_project_id=$1) as evidence_card_count,
           exists(select 1 from hepta_citation_records where paper_project_id=$1) as citation,
           (select count(*) from hepta_citation_records where paper_project_id=$1) as citation_count,
           exists(select 1 from hepta_experiment_plans where paper_project_id=$1) as experiment_plan,
           (select count(*) from hepta_experiment_plans where paper_project_id=$1) as experiment_plan_count,
           exists(select 1 from hepta_run_records where paper_project_id=$1) as retained_run,
           (select count(*) from hepta_run_records where paper_project_id=$1) as retained_run_count,
           (select count(*) from hepta_run_records where paper_project_id=$1 and status='succeeded') as successful_run_count,
           (select count(*) from hepta_run_records where paper_project_id=$1 and status='failed' and record_json->>'failure_hash' is not null) as retained_failed_run_count,
           exists(select 1 from hepta_claim_records where paper_project_id=$1) as claim,
           (select count(*) from hepta_claim_records where paper_project_id=$1) as claim_count,
           exists(select 1 from hepta_section_revisions where paper_project_id=$1) as section_revision,
           (select count(*) from hepta_section_revisions where paper_project_id=$1) as section_revision_count,
           exists(
             select 1 from hepta_section_reviews
             where paper_project_id=$1 and verdict='approve'
           ) as approving_section_review,
           (select count(*) from hepta_section_reviews where paper_project_id=$1 and verdict='approve') as approving_section_review_count,
           exists(select 1 from hepta_section_merges where paper_project_id=$1) as section_merge,
           (select count(*) from hepta_section_merges where paper_project_id=$1) as section_merge_count",
    )
    .bind(paper.paper_project_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    let paper_revision_covers_section_merges =
        collaboration_v3::paper_revision_covers_section_merges_postgres(tx, paper).await?;
    Ok(AuthorPhaseGateFacts {
        work_item: row.get("work_item"),
        work_item_count: u32::try_from(row.get::<i64, _>("work_item_count"))
            .map_err(|_| ApiError::internal("work item count is invalid"))?,
        accepted_work_item_count: u32::try_from(row.get::<i64, _>("accepted_work_item_count"))
            .map_err(|_| ApiError::internal("accepted work item count is invalid"))?,
        all_work_items_terminal: row.get("all_work_items_terminal"),
        paper_revision: paper.current_revision_id.is_some() && row.get("paper_revision"),
        paper_revision_count: u32::try_from(row.get::<i64, _>("paper_revision_count"))
            .map_err(|_| ApiError::internal("paper revision count is invalid"))?,
        paper_revision_covers_section_merges,
        collaboration: collaboration_v3::CollaborationPhaseGateFacts {
            artifact_manifest: row.get("artifact_manifest"),
            evidence_card: row.get("evidence_card"),
            citation: row.get("citation"),
            experiment_plan: row.get("experiment_plan"),
            retained_run: row.get("retained_run"),
            claim: row.get("claim"),
            section_revision: row.get("section_revision"),
            approving_section_review: row.get("approving_section_review"),
            section_merge: row.get("section_merge"),
        },
        collaboration_counts: collaboration_v3::CollaborationPhaseGateCounts {
            artifact_manifests: u32::try_from(row.get::<i64, _>("artifact_manifest_count"))
                .map_err(|_| ApiError::internal("artifact manifest count is invalid"))?,
            evidence_cards: u32::try_from(row.get::<i64, _>("evidence_card_count"))
                .map_err(|_| ApiError::internal("evidence card count is invalid"))?,
            citations: u32::try_from(row.get::<i64, _>("citation_count"))
                .map_err(|_| ApiError::internal("citation count is invalid"))?,
            experiment_plans: u32::try_from(row.get::<i64, _>("experiment_plan_count"))
                .map_err(|_| ApiError::internal("experiment plan count is invalid"))?,
            retained_runs: u32::try_from(row.get::<i64, _>("retained_run_count"))
                .map_err(|_| ApiError::internal("retained run count is invalid"))?,
            successful_runs: u32::try_from(row.get::<i64, _>("successful_run_count"))
                .map_err(|_| ApiError::internal("successful run count is invalid"))?,
            retained_failed_runs: u32::try_from(row.get::<i64, _>("retained_failed_run_count"))
                .map_err(|_| ApiError::internal("retained failed run count is invalid"))?,
            claims: u32::try_from(row.get::<i64, _>("claim_count"))
                .map_err(|_| ApiError::internal("claim count is invalid"))?,
            section_revisions: u32::try_from(row.get::<i64, _>("section_revision_count"))
                .map_err(|_| ApiError::internal("section revision count is invalid"))?,
            approving_section_reviews: u32::try_from(
                row.get::<i64, _>("approving_section_review_count"),
            )
            .map_err(|_| ApiError::internal("approving section review count is invalid"))?,
            section_merges: u32::try_from(row.get::<i64, _>("section_merge_count"))
                .map_err(|_| ApiError::internal("section merge count is invalid"))?,
        },
    })
}

fn ensure_author_phase_gate(
    paper: &PaperProject,
    next_phase: PaperPhase,
    facts: AuthorPhaseGateFacts,
) -> Result<(), ApiError> {
    let blockers = author_phase_gate_blockers(paper, next_phase, facts)?;
    if blockers.is_empty() {
        return Ok(());
    }
    Err(ApiError::conflict(
        "paper_phase_gate_blocked",
        format!(
            "paper phase cannot advance from {} to {} until these server-derived gates pass: {}",
            paper.phase.as_str(),
            next_phase.as_str(),
            blockers.join(",")
        ),
    ))
}

fn challenge_victory_gate_blockers(
    paper: &PaperProject,
    facts: AuthorPhaseGateFacts,
    release_candidate: bool,
    all_author_consents: bool,
) -> Result<Vec<String>, ApiError> {
    let Some(ruleset) = authoritative_paper_ruleset(paper)? else {
        // The historical lane remains scientifically reproducible but is
        // explicitly legacy-unranked; existing finalization hard gates below
        // continue to apply without inventing a typed ruleset retroactively.
        return Ok(Vec::new());
    };
    Ok(typed_requirement_blockers(
        &ruleset.victory_requirements,
        facts,
        release_candidate,
        all_author_consents,
    ))
}

fn ensure_challenge_victory_gate(
    paper: &PaperProject,
    facts: AuthorPhaseGateFacts,
    release_candidate: bool,
    all_author_consents: bool,
) -> Result<(), ApiError> {
    let blockers =
        challenge_victory_gate_blockers(paper, facts, release_candidate, all_author_consents)?;
    if blockers.is_empty() {
        return Ok(());
    }
    Err(ApiError::conflict(
        "challenge_victory_gate_blocked",
        format!(
            "paper cannot finish this immutable challenge ruleset until these victory minimums pass: {}",
            blockers.join(",")
        ),
    ))
}

async fn transition_paper(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<TransitionPaperRequest>,
) -> Result<(StatusCode, Json<PaperProject>), ApiError> {
    const OPERATION: &str = "transition_paper_project_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/transition");
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
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        let paper_snapshot = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        ensure_paper_gameplay_active(&paper_snapshot, Utc::now())?;
        let team = memory
            .teams
            .get(&paper_snapshot.team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        require_author_role(
            team,
            assertion.player_id,
            "captain",
            "paper phase transition",
        )?;
        let phase_gate_facts = author_phase_gate_facts_memory(&memory, &paper_snapshot);
        let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
        if paper.version != request.expected_version {
            return Err(version_conflict(
                "paper project",
                request.expected_version,
                paper.version,
            ));
        }
        if !paper.phase.can_transition_to(request.next_phase) {
            return Err(ApiError::conflict(
                "invalid_paper_phase_transition",
                format!(
                    "paper phase cannot transition from {} to {}",
                    paper.phase.as_str(),
                    request.next_phase.as_str()
                ),
            ));
        }
        ensure_author_phase_gate(paper, request.next_phase, phase_gate_facts)?;
        paper.phase = request.next_phase;
        paper.version += 1;
        paper.updated_at = Utc::now();
        let response = paper.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.paper_project.phase_changed.v2",
            response.paper_project_id,
            response.version,
            json!({
                "paper_project_id": response.paper_project_id,
                "phase": response.phase,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let now = collaboration_v3::postgres_transaction_now(&mut tx).await?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(row.get("record_json"), "paper project")?;
    ensure_paper_gameplay_active(&paper, now)?;
    assert_team_actor_postgres(&mut tx, paper.team_id, &assertion).await?;
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    require_author_role(
        &team,
        assertion.player_id,
        "captain",
        "paper phase transition",
    )?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if actual_version != request.expected_version {
        return Err(version_conflict(
            "paper project",
            request.expected_version,
            actual_version,
        ));
    }
    if !paper.phase.can_transition_to(request.next_phase) {
        return Err(ApiError::conflict(
            "invalid_paper_phase_transition",
            format!(
                "paper phase cannot transition from {} to {}",
                paper.phase.as_str(),
                request.next_phase.as_str()
            ),
        ));
    }
    let phase_gate_facts = author_phase_gate_facts_postgres(&mut tx, &paper).await?;
    ensure_author_phase_gate(&paper, request.next_phase, phase_gate_facts)?;
    paper.phase = request.next_phase;
    paper.version += 1;
    paper.updated_at = now;
    let record_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set phase = $1, outcome = $2, outcome_reason = $3, terminal_at = $4,
             version = $5, record_json = $6::jsonb, updated_at = $7
         where paper_project_id = $8 and version = $9",
    )
    .bind(paper.phase.as_str())
    .bind(paper.outcome.as_str())
    .bind(&paper.outcome_reason)
    .bind(paper.terminal_at)
    .bind(paper.version as i64)
    .bind(record_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_version as i64)
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
        "hepta.paper_raid.paper_project.phase_changed.v2",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "phase": paper.phase,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper.paper_project_id),
        StatusCode::OK,
        &paper,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(paper)))
}

fn validate_assignment(
    assigned_player_id: Option<Uuid>,
    assigned_binding_id: Option<Uuid>,
) -> Result<(), ApiError> {
    if assigned_player_id.is_some() != assigned_binding_id.is_some() {
        return Err(ApiError::bad_request(
            "invalid_work_item_assignment",
            "assigned_player_id and assigned_binding_id must both be present or both be absent",
        ));
    }
    Ok(())
}

fn can_transition_work_item(current: WorkItemStatus, next: WorkItemStatus) -> bool {
    matches!(
        (current, next),
        (WorkItemStatus::Planned, WorkItemStatus::InProgress)
            | (WorkItemStatus::Planned, WorkItemStatus::Cancelled)
            | (WorkItemStatus::InProgress, WorkItemStatus::Review)
            | (WorkItemStatus::InProgress, WorkItemStatus::Cancelled)
            | (WorkItemStatus::Review, WorkItemStatus::Accepted)
            | (WorkItemStatus::Review, WorkItemStatus::Rejected)
            | (WorkItemStatus::Rejected, WorkItemStatus::InProgress)
    )
}

async fn create_work_item(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemRequest>,
) -> Result<(StatusCode, Json<WorkItem>), ApiError> {
    const OPERATION: &str = "create_paper_work_item_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_assignment(request.assigned_player_id, request.assigned_binding_id)?;
    validate_contract_text_api("kind", &request.kind)?;
    validate_contract_text_api("title", &request.title)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/work-items");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let now = Utc::now();

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        if memory.work_items.contains_key(&request.work_item_id) {
            return Err(ApiError::conflict(
                "work_item_conflict",
                "work_item_id already exists",
            ));
        }
        let paper_snapshot = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        ensure_paper_gameplay_active(&paper_snapshot, now)?;
        let (team_id, paper_version, phase) = (
            paper_snapshot.team_id,
            paper_snapshot.version,
            paper_snapshot.phase,
        );
        let team = memory
            .teams
            .get(&team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        require_captain_or_self_assignment(team, assertion.player_id, request.assigned_player_id)?;
        if paper_version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper_version,
            ));
        }
        if phase == PaperPhase::SubmissionReady {
            return Err(ApiError::conflict(
                "paper_project_finalized",
                "finalized paper projects cannot accept new work items",
            ));
        }
        if let (Some(player_id), Some(binding_id)) =
            (request.assigned_player_id, request.assigned_binding_id)
        {
            let team = memory.teams.get(&team_id).expect("paper team exists");
            if !team
                .members
                .iter()
                .any(|member| member.player_id == player_id && member.binding_id == binding_id)
            {
                return Err(ApiError::conflict(
                    "work_item_assignee_not_on_team",
                    "work item assignee and binding must belong to the paper team",
                ));
            }
        }
        let item = WorkItem {
            work_item_id: request.work_item_id,
            paper_project_id: paper_id,
            kind: request.kind.clone(),
            title: request.title.clone(),
            assigned_player_id: request.assigned_player_id,
            assigned_binding_id: request.assigned_binding_id,
            status: WorkItemStatus::Planned,
            artifact_manifest_hash: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        memory.work_items.insert(item.work_item_id, item.clone());
        let new_paper_version = {
            let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
            paper.version += 1;
            paper.updated_at = now;
            paper.version
        };
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.work_item.created.v2",
            paper_id,
            paper_id,
            new_paper_version,
            json!({
                "paper_project_id": paper_id,
                "work_item_id": item.work_item_id,
                "kind": item.kind,
            }),
        );
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &item,
        )?;
        return Ok((StatusCode::CREATED, Json(item)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select team_id, phase, version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    ensure_paper_gameplay_active(&paper, now)?;
    assert_team_actor_postgres(&mut tx, paper.team_id, &assertion).await?;
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    require_captain_or_self_assignment(&team, assertion.player_id, request.assigned_player_id)?;
    let actual_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if actual_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            actual_version,
        ));
    }
    if paper.phase == PaperPhase::SubmissionReady {
        return Err(ApiError::conflict(
            "paper_project_finalized",
            "finalized paper projects cannot accept new work items",
        ));
    }
    if let (Some(player_id), Some(binding_id)) =
        (request.assigned_player_id, request.assigned_binding_id)
    {
        let member_exists = sqlx::query(
            "select 1 from hepta_research_team_members
             where team_id = $1 and player_id = $2 and binding_id = $3",
        )
        .bind(paper.team_id)
        .bind(player_id)
        .bind(binding_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .is_some();
        if !member_exists {
            return Err(ApiError::conflict(
                "work_item_assignee_not_on_team",
                "work item assignee and binding must belong to the paper team",
            ));
        }
    }
    let item = WorkItem {
        work_item_id: request.work_item_id,
        paper_project_id: paper_id,
        kind: request.kind.clone(),
        title: request.title.clone(),
        assigned_player_id: request.assigned_player_id,
        assigned_binding_id: request.assigned_binding_id,
        status: WorkItemStatus::Planned,
        artifact_manifest_hash: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let item_json = serde_json::to_value(&item)
        .map_err(|error| ApiError::internal(format!("encode work item: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_paper_work_items (
            work_item_id, paper_project_id, assigned_player_id, assigned_binding_id,
            status, version, record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8,$9)",
    )
    .bind(item.work_item_id)
    .bind(item.paper_project_id)
    .bind(item.assigned_player_id)
    .bind(item.assigned_binding_id)
    .bind(item.status.as_str())
    .bind(item.version as i64)
    .bind(item_json)
    .bind(item.created_at)
    .bind(item.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "work_item_conflict",
                "work_item_id already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    paper.version += 1;
    paper.updated_at = now;
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set version = $1, record_json = $2::jsonb, updated_at = $3
         where paper_project_id = $4 and version = $5",
    )
    .bind(paper.version as i64)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_paper_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "paper project changed concurrently",
        ));
    }
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.work_item.created.v2",
        paper.paper_project_id,
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "work_item_id": item.work_item_id,
            "kind": item.kind,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(item.work_item_id),
        StatusCode::CREATED,
        &item,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn transition_work_item(
    State(state): State<AppState>,
    Path(work_item_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<TransitionWorkItemRequest>,
) -> Result<(StatusCode, Json<WorkItem>), ApiError> {
    const OPERATION: &str = "transition_paper_work_item_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    if let Some(hash) = &request.artifact_manifest_hash {
        validate_digest_v2("artifact_manifest_hash", hash)?;
    }
    if request.next_status == WorkItemStatus::Accepted && request.artifact_manifest_hash.is_none() {
        return Err(ApiError::bad_request(
            "artifact_manifest_required",
            "accepted work items require an artifact_manifest_hash",
        ));
    }
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/work-items/{work_item_id}/transition");
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
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let item_snapshot = memory
            .work_items
            .get(&work_item_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("work_item_not_found", "work item does not exist")
            })?;
        let paper = memory
            .papers
            .get(&item_snapshot.paper_project_id)
            .ok_or_else(|| ApiError::internal("work item paper project is missing"))?;
        ensure_paper_gameplay_active(paper, Utc::now())?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .ok_or_else(|| ApiError::internal("work item research team is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        require_captain_or_work_item_assignee(
            team,
            assertion.player_id,
            item_snapshot.assigned_player_id,
        )?;
        let item = memory
            .work_items
            .get_mut(&work_item_id)
            .expect("work item exists");
        if item.version != request.expected_version {
            return Err(version_conflict(
                "work item",
                request.expected_version,
                item.version,
            ));
        }
        if !can_transition_work_item(item.status, request.next_status) {
            return Err(ApiError::conflict(
                "invalid_work_item_transition",
                "work item status transition is not allowed",
            ));
        }
        item.status = request.next_status;
        if request.artifact_manifest_hash.is_some() {
            item.artifact_manifest_hash = request.artifact_manifest_hash.clone();
        }
        item.version += 1;
        item.updated_at = Utc::now();
        let response = item.clone();
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.work_item.status_changed.v2",
            response.work_item_id,
            response.version,
            json!({
                "work_item_id": response.work_item_id,
                "paper_project_id": response.paper_project_id,
                "status": response.status,
                "artifact_manifest_hash": response.artifact_manifest_hash,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let row = sqlx::query(
        "select version, record_json from hepta_paper_work_items
         where work_item_id = $1 for update",
    )
    .bind(work_item_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::not_found("work_item_not_found", "work item does not exist"))?;
    let mut item: WorkItem = decode_record(row.get("record_json"), "work item")?;
    let paper_row = sqlx::query(
        "select team_id,record_json from hepta_paper_projects where paper_project_id = $1 for share",
    )
    .bind(item.paper_project_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let team_id: Uuid = paper_row.get("team_id");
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    ensure_paper_gameplay_active(&paper, Utc::now())?;
    assert_team_actor_postgres(&mut tx, team_id, &assertion).await?;
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for share")
            .bind(team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    require_captain_or_work_item_assignee(&team, assertion.player_id, item.assigned_player_id)?;
    let actual_version = u64::try_from(row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("work item version is invalid"))?;
    if actual_version != request.expected_version {
        return Err(version_conflict(
            "work item",
            request.expected_version,
            actual_version,
        ));
    }
    if !can_transition_work_item(item.status, request.next_status) {
        return Err(ApiError::conflict(
            "invalid_work_item_transition",
            "work item status transition is not allowed",
        ));
    }
    item.status = request.next_status;
    if request.artifact_manifest_hash.is_some() {
        item.artifact_manifest_hash = request.artifact_manifest_hash.clone();
    }
    item.version += 1;
    item.updated_at = Utc::now();
    let record_json = serde_json::to_value(&item)
        .map_err(|error| ApiError::internal(format!("encode work item: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_work_items
         set status = $1, version = $2, record_json = $3::jsonb, updated_at = $4
         where work_item_id = $5 and version = $6",
    )
    .bind(item.status.as_str())
    .bind(item.version as i64)
    .bind(record_json)
    .bind(item.updated_at)
    .bind(item.work_item_id)
    .bind(request.expected_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "work item changed concurrently",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.work_item.status_changed.v2",
        item.work_item_id,
        item.version,
        json!({
            "work_item_id": item.work_item_id,
            "paper_project_id": item.paper_project_id,
            "status": item.status,
            "artifact_manifest_hash": item.artifact_manifest_hash,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(item.work_item_id),
        StatusCode::OK,
        &item,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(item)))
}

fn validate_revision_hashes(request: &CreatePaperRevisionRequest) -> Result<(), ApiError> {
    validate_digest_v2("source_manifest_hash", &request.source_manifest_hash)?;
    validate_digest_v2("artifact_manifest_hash", &request.artifact_manifest_hash)?;
    validate_digest_v2("bibliography_hash", &request.bibliography_hash)?;
    validate_digest_v2(
        "claim_evidence_graph_hash",
        &request.claim_evidence_graph_hash,
    )?;
    Ok(())
}

async fn create_revision(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreatePaperRevisionRequest>,
) -> Result<(StatusCode, Json<PaperRevision>), ApiError> {
    const OPERATION: &str = "create_paper_revision_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_revision_hashes(&request)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/revisions");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
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
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        if memory.revisions.contains_key(&request.revision_id) {
            return Err(ApiError::conflict(
                "paper_revision_conflict",
                "revision_id already exists",
            ));
        }
        let paper_snapshot = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        ensure_paper_gameplay_active(&paper_snapshot, now)?;
        let (paper_version, phase, current_revision_id, team_id) = (
            paper_snapshot.version,
            paper_snapshot.phase,
            paper_snapshot.current_revision_id,
            paper_snapshot.team_id,
        );
        let team = memory
            .teams
            .get(&team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, team, &assertion)?;
        if paper_version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper_version,
            ));
        }
        if !matches!(phase, PaperPhase::Drafting | PaperPhase::Reproducing) {
            return Err(ApiError::conflict(
                "paper_revision_phase_closed",
                "paper revisions can only be created during drafting or reproducing",
            ));
        }
        if request.parent_revision_id != current_revision_id {
            return Err(ApiError::conflict(
                "stale_parent_revision",
                "parent_revision_id must equal the current paper revision",
            ));
        }
        let materialization = collaboration_v3::resolve_revision_artifact_binding_memory(
            &memory, paper_id, &request,
        )?;
        let revision_number = if let Some(parent_id) = current_revision_id {
            let parent = memory
                .revisions
                .get_mut(&parent_id)
                .ok_or_else(|| ApiError::internal("paper current revision record is missing"))?;
            if parent.paper_project_id != paper_id {
                return Err(ApiError::conflict(
                    "cross_paper_parent_revision",
                    "parent revision belongs to a different paper project",
                ));
            }
            let number = parent.revision_number + 1;
            parent.status = PaperRevisionStatus::Superseded;
            parent.version += 1;
            parent.updated_at = now;
            number
        } else {
            1
        };
        let revision = PaperRevision {
            revision_id: request.revision_id,
            paper_project_id: paper_id,
            parent_revision_id: request.parent_revision_id,
            revision_number,
            source_manifest_hash: request.source_manifest_hash.clone(),
            artifact_manifest_hash: request.artifact_manifest_hash.clone(),
            bibliography_hash: request.bibliography_hash.clone(),
            claim_evidence_graph_hash: request.claim_evidence_graph_hash.clone(),
            section_materialization: Some(materialization.descriptor.clone()),
            section_materialization_root: Some(materialization.root.clone()),
            status: PaperRevisionStatus::Draft,
            release_candidate: None,
            release_candidate_hash: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        memory
            .revisions
            .insert(revision.revision_id, revision.clone());
        collaboration_v3::store_revision_artifact_binding_memory(
            &mut memory,
            materialization.artifact_binding,
        )?;
        collaboration_v3::rebase_section_heads_memory(
            &mut memory,
            paper_id,
            revision.revision_id,
            materialization.section_head_count,
            now,
        )?;
        let new_paper_version = {
            let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
            paper.current_revision_id = Some(revision.revision_id);
            paper.release_candidate_revision_id = None;
            paper.version += 1;
            paper.updated_at = now;
            paper.version
        };
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.paper_revision.created.v2",
            paper_id,
            new_paper_version,
            json!({
                "paper_project_id": paper_id,
                "revision_id": revision.revision_id,
                "revision_number": revision.revision_number,
                "parent_revision_id": revision.parent_revision_id,
                "section_materialization_root": revision.section_materialization_root,
                "materialized_section_count": revision.section_materialization
                    .as_ref().map(|descriptor| descriptor.sections.len()).unwrap_or(0),
            }),
        )?;
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
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    ensure_paper_gameplay_active(&paper, now)?;
    assert_team_actor_postgres(&mut tx, paper.team_id, &assertion).await?;
    let actual_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if actual_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            actual_version,
        ));
    }
    if !matches!(paper.phase, PaperPhase::Drafting | PaperPhase::Reproducing) {
        return Err(ApiError::conflict(
            "paper_revision_phase_closed",
            "paper revisions can only be created during drafting or reproducing",
        ));
    }
    if request.parent_revision_id != paper.current_revision_id {
        return Err(ApiError::conflict(
            "stale_parent_revision",
            "parent_revision_id must equal the current paper revision",
        ));
    }
    let materialization =
        collaboration_v3::resolve_revision_artifact_binding_postgres(&mut tx, paper_id, &request)
            .await?;
    let revision_number = if let Some(parent_id) = paper.current_revision_id {
        let parent_row = sqlx::query(
            "select record_json from hepta_paper_revisions
             where revision_id = $1 and paper_project_id = $2 for update",
        )
        .bind(parent_id)
        .bind(paper_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::internal("paper current revision record is missing"))?;
        let mut parent: PaperRevision =
            decode_record(parent_row.get("record_json"), "parent revision")?;
        let number = parent.revision_number + 1;
        parent.status = PaperRevisionStatus::Superseded;
        parent.version += 1;
        parent.updated_at = now;
        let parent_json = serde_json::to_value(&parent)
            .map_err(|error| ApiError::internal(format!("encode parent revision: {error}")))?;
        sqlx::query(
            "update hepta_paper_revisions
             set status = $1, version = $2, record_json = $3::jsonb, updated_at = $4
             where revision_id = $5 and paper_project_id = $6",
        )
        .bind(parent.status.as_str())
        .bind(parent.version as i64)
        .bind(parent_json)
        .bind(parent.updated_at)
        .bind(parent.revision_id)
        .bind(parent.paper_project_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        number
    } else {
        1
    };
    let revision = PaperRevision {
        revision_id: request.revision_id,
        paper_project_id: paper_id,
        parent_revision_id: request.parent_revision_id,
        revision_number,
        source_manifest_hash: request.source_manifest_hash.clone(),
        artifact_manifest_hash: request.artifact_manifest_hash.clone(),
        bibliography_hash: request.bibliography_hash.clone(),
        claim_evidence_graph_hash: request.claim_evidence_graph_hash.clone(),
        section_materialization: Some(materialization.descriptor.clone()),
        section_materialization_root: Some(materialization.root.clone()),
        status: PaperRevisionStatus::Draft,
        release_candidate: None,
        release_candidate_hash: None,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    let revision_json = serde_json::to_value(&revision)
        .map_err(|error| ApiError::internal(format!("encode paper revision: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_paper_revisions (
            revision_id, paper_project_id, parent_revision_id, revision_number,
            status, release_candidate_hash, version, record_json, created_at, updated_at
         ) values ($1,$2,$3,$4,$5,null,$6,$7::jsonb,$8,$9)",
    )
    .bind(revision.revision_id)
    .bind(revision.paper_project_id)
    .bind(revision.parent_revision_id)
    .bind(revision.revision_number as i64)
    .bind(revision.status.as_str())
    .bind(revision.version as i64)
    .bind(revision_json)
    .bind(revision.created_at)
    .bind(revision.updated_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "paper_revision_conflict",
                "revision_id or revision_number already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    collaboration_v3::store_revision_artifact_binding_postgres(
        &mut tx,
        &materialization.artifact_binding,
    )
    .await?;
    collaboration_v3::rebase_section_heads_postgres(
        &mut tx,
        paper_id,
        revision.revision_id,
        materialization.section_head_count,
        now,
    )
    .await?;
    paper.current_revision_id = Some(revision.revision_id);
    paper.release_candidate_revision_id = None;
    paper.version += 1;
    paper.updated_at = now;
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set version = $1, record_json = $2::jsonb, updated_at = $3
         where paper_project_id = $4 and version = $5",
    )
    .bind(paper.version as i64)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_paper_version as i64)
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
        "hepta.paper_raid.paper_revision.created.v2",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "revision_id": revision.revision_id,
            "revision_number": revision.revision_number,
            "parent_revision_id": revision.parent_revision_id,
            "section_materialization_root": revision.section_materialization_root,
            "materialized_section_count": revision.section_materialization
                .as_ref().map(|descriptor| descriptor.sections.len()).unwrap_or(0),
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(revision.revision_id),
        StatusCode::CREATED,
        &revision,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(revision)))
}

fn validated_revision_materialization_root(revision: &PaperRevision) -> Result<String, ApiError> {
    let descriptor = revision.section_materialization.as_ref().ok_or_else(|| {
        ApiError::conflict(
            "section_materialization_required",
            "new release candidates require a rooted whole-paper section materialization",
        )
    })?;
    let root = revision
        .section_materialization_root
        .as_ref()
        .ok_or_else(|| {
            ApiError::conflict(
                "section_materialization_required",
                "new release candidates require a rooted whole-paper section materialization",
            )
        })?;
    if descriptor.paper_project_id != revision.paper_project_id
        || descriptor.revision_id != revision.revision_id
        || descriptor.parent_revision_id != revision.parent_revision_id
        || section_materialization_root(descriptor).ok().as_ref() != Some(root)
    {
        return Err(ApiError::conflict(
            "section_materialization_mismatch",
            "paper revision section materialization descriptor/root is inconsistent",
        ));
    }
    Ok(root.clone())
}

fn validate_release_authors(
    team: &ResearchTeam,
    players: &HashMap<Uuid, HumanPlayer>,
    authors: &[PaperReleaseAuthorV2],
) -> Result<(), ApiError> {
    if authors.len() != team.members.len() {
        return Err(ApiError::bad_request(
            "author_roster_mismatch",
            "release candidate must include every team member exactly once",
        ));
    }
    let mut ordered = authors.to_vec();
    ordered.sort_by_key(|author| author.participant_slot);
    for (member, author) in team.members.iter().zip(ordered.iter()) {
        let player = players
            .get(&member.player_id)
            .ok_or_else(|| ApiError::internal("team member human player record is missing"))?;
        if author.participant_slot != member.participant_slot
            || author.player_id != member.player_id
            || author.display_name != player.display_name
        {
            return Err(ApiError::bad_request(
                "author_roster_mismatch",
                "release candidate authors must exactly match the locked team roster",
            ));
        }
    }
    Ok(())
}

fn validate_contribution_ledger_id(contribution_ledger_id: Uuid) -> Result<(), ApiError> {
    if contribution_ledger_id.is_nil() {
        return Err(ApiError::bad_request(
            "nil_contribution_ledger_id",
            "contribution_ledger_id must not be the nil UUID",
        ));
    }
    Ok(())
}

fn reserve_contribution_ledger_id_memory(
    memory: &mut PaperRaidMemory,
    contribution_ledger_id: Uuid,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<(), ApiError> {
    validate_contribution_ledger_id(contribution_ledger_id)?;
    if let Some(existing) = memory
        .contribution_ledger_reservations
        .get(&contribution_ledger_id)
    {
        if existing.paper_project_id == paper_id
            && existing.release_candidate_hash == release_candidate_hash
        {
            return Ok(());
        }
        return Err(ApiError::conflict(
            "contribution_ledger_id_reserved",
            "contribution_ledger_id is already reserved by another Paper release candidate",
        ));
    }
    if memory
        .review
        .contribution_ledgers
        .get(&contribution_ledger_id)
        .is_some_and(|ledger| {
            ledger.paper_project_id != paper_id
                || ledger.release_candidate_hash != release_candidate_hash
        })
    {
        return Err(ApiError::conflict(
            "contribution_ledger_id_reserved",
            "contribution_ledger_id is already owned by another frozen contribution ledger",
        ));
    }
    memory.contribution_ledger_reservations.insert(
        contribution_ledger_id,
        ContributionLedgerReservation {
            paper_project_id: paper_id,
            release_candidate_hash: release_candidate_hash.to_string(),
        },
    );
    Ok(())
}

fn require_contribution_ledger_reservation_memory(
    memory: &PaperRaidMemory,
    contribution_ledger_id: Uuid,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<(), ApiError> {
    validate_contribution_ledger_id(contribution_ledger_id)?;
    match memory
        .contribution_ledger_reservations
        .get(&contribution_ledger_id)
    {
        Some(reservation)
            if reservation.paper_project_id == paper_id
                && reservation.release_candidate_hash == release_candidate_hash =>
        {
            Ok(())
        }
        Some(_) => Err(ApiError::conflict(
            "contribution_ledger_id_ownership_mismatch",
            "contribution_ledger_id is reserved by another Paper release candidate",
        )),
        None => Err(ApiError::conflict(
            "contribution_ledger_id_not_reserved",
            "contribution_ledger_id must be atomically reserved during release-candidate promotion",
        )),
    }
}

async fn reserve_contribution_ledger_id_postgres(
    tx: &mut Transaction<'_, Postgres>,
    contribution_ledger_id: Uuid,
    paper_id: Uuid,
    release_candidate_hash: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    validate_contribution_ledger_id(contribution_ledger_id)?;
    let inserted = sqlx::query(
        "insert into hepta_paper_contribution_ledger_reservations
         (contribution_ledger_id,paper_project_id,release_candidate_hash,created_at)
         values ($1,$2,$3,$4) on conflict do nothing",
    )
    .bind(contribution_ledger_id)
    .bind(paper_id)
    .bind(release_candidate_hash)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if inserted.rows_affected() == 1 {
        return Ok(());
    }
    let existing = sqlx::query(
        "select paper_project_id,release_candidate_hash
         from hepta_paper_contribution_ledger_reservations
         where contribution_ledger_id=$1 for key share",
    )
    .bind(contribution_ledger_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if existing.is_some_and(|row| {
        row.get::<Uuid, _>("paper_project_id") == paper_id
            && row.get::<String, _>("release_candidate_hash") == release_candidate_hash
    }) {
        return Ok(());
    }
    Err(ApiError::conflict(
        "contribution_ledger_id_reserved",
        "contribution_ledger_id is already reserved by another Paper release candidate",
    ))
}

async fn require_contribution_ledger_reservation_postgres(
    tx: &mut Transaction<'_, Postgres>,
    contribution_ledger_id: Uuid,
    paper_id: Uuid,
    release_candidate_hash: &str,
) -> Result<(), ApiError> {
    validate_contribution_ledger_id(contribution_ledger_id)?;
    let row = sqlx::query(
        "select paper_project_id,release_candidate_hash
         from hepta_paper_contribution_ledger_reservations
         where contribution_ledger_id=$1 for key share",
    )
    .bind(contribution_ledger_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    match row {
        Some(row)
            if row.get::<Uuid, _>("paper_project_id") == paper_id
                && row.get::<String, _>("release_candidate_hash") == release_candidate_hash =>
        {
            Ok(())
        }
        Some(_) => Err(ApiError::conflict(
            "contribution_ledger_id_ownership_mismatch",
            "contribution_ledger_id is reserved by another Paper release candidate",
        )),
        None => Err(ApiError::conflict(
            "contribution_ledger_id_not_reserved",
            "contribution_ledger_id must be atomically reserved during release-candidate promotion",
        )),
    }
}

async fn promote_release_candidate(
    State(state): State<AppState>,
    Path((paper_id, revision_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<PromoteReleaseCandidateRequest>,
) -> Result<(StatusCode, Json<PromoteReleaseCandidateResponse>), ApiError> {
    const OPERATION: &str = "promote_paper_release_candidate_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_contribution_ledger_id(request.contribution_ledger_id)?;
    validate_contract_text_api("title", &request.title)?;
    validate_contract_text_api("abstract_text", &request.abstract_text)?;
    validate_contract_text_api("license", &request.license)?;
    validate_digest_v2(
        "collaboration_compact_hash",
        &request.collaboration_compact_hash,
    )?;
    validate_digest_v2(
        "research_protocol_snapshot_hash",
        &request.research_protocol_snapshot_hash,
    )?;
    validate_digest_v2("ethics_disclosure_hash", &request.ethics_disclosure_hash)?;
    validate_digest_v2("coi_disclosure_hash", &request.coi_disclosure_hash)?;
    validate_digest_v2(
        "contribution_ledger_hash",
        &request.contribution_ledger_hash,
    )?;
    validate_digest_v2("ai_disclosure_hash", &request.ai_disclosure_hash)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/revisions/{revision_id}/promote");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
        &request.idempotency_key,
        &request_hash,
    )?;
    let challenges = state
        .inspect(|league| Ok(league.challenges.clone()))
        .await?;
    let now = Utc::now();

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        ensure_paper_gameplay_active(&paper, now)?;
        if paper.phase != PaperPhase::AuthorApproval
            || paper.current_revision_id != Some(revision_id)
        {
            return Err(ApiError::conflict(
                "release_candidate_not_promotable",
                "release candidate must be the current revision in author_approval",
            ));
        }
        if paper.release_candidate_revision_id.is_some() {
            return Err(ApiError::conflict(
                "release_candidate_exists",
                "paper project already has a release candidate",
            ));
        }
        let team = memory
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, &team, &assertion)?;
        if team.status != TeamStatus::Locked
            || team.challenge_id != paper.challenge_id
            || !(3..=5).contains(&team.members.len())
        {
            return Err(ApiError::conflict(
                "paper_team_invariant_failed",
                "paper team must remain locked with 3-5 members on the same challenge",
            ));
        }
        if request.collaboration_compact_hash != team.collaboration_compact_hash {
            return Err(ApiError::conflict(
                "collaboration_compact_mismatch",
                "release candidate must bind the compact accepted by every team member",
            ));
        }
        let challenge = challenges
            .get(&paper.challenge_id)
            .ok_or_else(|| ApiError::internal("paper challenge record is missing"))?;
        let (ruleset_hash, challenge_snapshot_hash) =
            paper_challenge_binding_hashes(&paper, challenge)?;
        validate_release_authors(&team, &memory.players, &request.authors)?;
        let revision = memory.revisions.get(&revision_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
        })?;
        if revision.paper_project_id != paper_id
            || revision.status != PaperRevisionStatus::Draft
            || revision.version != request.expected_revision_version
        {
            return Err(ApiError::conflict(
                "paper_revision_version_conflict",
                "paper revision is not the expected draft version for this project",
            ));
        }
        let contribution_entries = review_v4::authoritative_contribution_entries_memory(
            &memory,
            paper_id,
            &request.authors,
        )?;
        let authoritative_contribution_hash = review_v4::contribution_ledger_hash(
            request.contribution_ledger_id,
            paper_id,
            &contribution_entries,
        )?;
        if request.contribution_ledger_hash != authoritative_contribution_hash {
            return Err(ApiError::conflict(
                "contribution_ledger_hash_mismatch",
                "release candidate must bind the complete authoritative contribution ledger",
            ));
        }
        let section_materialization_root = validated_revision_materialization_root(&revision)?;
        let candidate = PaperReleaseCandidateV2 {
            schema: PAPER_RELEASE_CANDIDATE_V2.to_string(),
            paper_project_id: paper.paper_project_id,
            revision_id,
            team_id: paper.team_id,
            challenge_id: paper.challenge_id,
            ruleset_hash,
            challenge_snapshot_hash,
            roster_version: team.roster_version,
            title: request.title.clone(),
            abstract_text: request.abstract_text.clone(),
            target_format: paper.target_format.clone(),
            source_manifest_hash: revision.source_manifest_hash.clone(),
            artifact_manifest_hash: revision.artifact_manifest_hash.clone(),
            bibliography_hash: revision.bibliography_hash.clone(),
            claim_evidence_graph_hash: revision.claim_evidence_graph_hash.clone(),
            section_materialization_root: Some(section_materialization_root),
            collaboration_compact_hash: request.collaboration_compact_hash.clone(),
            research_protocol_snapshot_hash: request.research_protocol_snapshot_hash.clone(),
            ethics_disclosure_hash: request.ethics_disclosure_hash.clone(),
            coi_disclosure_hash: request.coi_disclosure_hash.clone(),
            contribution_ledger_hash: request.contribution_ledger_hash.clone(),
            ai_disclosure_hash: request.ai_disclosure_hash.clone(),
            license: request.license.clone(),
            authors: request.authors.clone(),
        };
        let release_hash = paper_release_candidate_hash(&candidate)
            .map_err(|message| ApiError::bad_request("invalid_release_candidate", message))?;
        let mut next = memory.clone();
        reserve_contribution_ledger_id_memory(
            &mut next,
            request.contribution_ledger_id,
            paper_id,
            &release_hash,
        )?;
        let updated_revision = {
            let revision = next
                .revisions
                .get_mut(&revision_id)
                .expect("revision exists");
            revision.status = PaperRevisionStatus::ReleaseCandidate;
            revision.release_candidate = Some(candidate);
            revision.release_candidate_hash = Some(release_hash.clone());
            revision.version += 1;
            revision.updated_at = now;
            revision.clone()
        };
        let paper_version = {
            let paper = next.papers.get_mut(&paper_id).expect("paper exists");
            paper.release_candidate_revision_id = Some(revision_id);
            paper.version += 1;
            paper.updated_at = now;
            paper.version
        };
        let response = PromoteReleaseCandidateResponse {
            revision: updated_revision,
            release_candidate_hash: release_hash,
            paper_version,
        };
        push_memory_event(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.release_candidate.promoted.v2",
            paper_id,
            paper_version,
            json!({
                "paper_project_id": paper_id,
                "revision_id": revision_id,
                "release_candidate_hash": response.release_candidate_hash,
                "contribution_ledger_id": request.contribution_ledger_id,
                "roster_version": team.roster_version,
            }),
        )?;
        memory_remember(
            &mut next,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        *memory = next;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let paper_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if paper_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper_version,
        ));
    }
    ensure_paper_gameplay_active(&paper, now)?;
    if paper.phase != PaperPhase::AuthorApproval
        || paper.current_revision_id != Some(revision_id)
        || paper.release_candidate_revision_id.is_some()
    {
        return Err(ApiError::conflict(
            "release_candidate_not_promotable",
            "release candidate must be the sole current revision in author_approval",
        ));
    }
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id = $1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    assert_team_actor_postgres(&mut tx, team.team_id, &assertion).await?;
    if team.status != TeamStatus::Locked
        || team.challenge_id != paper.challenge_id
        || !(3..=5).contains(&team.members.len())
    {
        return Err(ApiError::conflict(
            "paper_team_invariant_failed",
            "paper team must remain locked with 3-5 members on the same challenge",
        ));
    }
    if request.collaboration_compact_hash != team.collaboration_compact_hash {
        return Err(ApiError::conflict(
            "collaboration_compact_mismatch",
            "release candidate must bind the compact accepted by every team member",
        ));
    }
    let challenge = challenges
        .get(&paper.challenge_id)
        .ok_or_else(|| ApiError::internal("paper challenge record is missing"))?;
    let (ruleset_hash, challenge_snapshot_hash) =
        paper_challenge_binding_hashes(&paper, challenge)?;
    let mut players = HashMap::new();
    for member in &team.members {
        let row = sqlx::query(
            "select record_json from hepta_human_players where player_id = $1 for share",
        )
        .bind(member.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        players.insert(
            member.player_id,
            decode_record(row.get("record_json"), "human player")?,
        );
    }
    validate_release_authors(&team, &players, &request.authors)?;
    let revision_row = sqlx::query(
        "select version, record_json from hepta_paper_revisions
         where revision_id = $1 and paper_project_id = $2 for update",
    )
    .bind(revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
    })?;
    let mut revision: PaperRevision =
        decode_record(revision_row.get("record_json"), "paper revision")?;
    let revision_version = u64::try_from(revision_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("revision version is invalid"))?;
    if revision.status != PaperRevisionStatus::Draft
        || revision_version != request.expected_revision_version
    {
        return Err(ApiError::conflict(
            "paper_revision_version_conflict",
            "paper revision is not the expected draft version",
        ));
    }
    let contribution_entries =
        review_v4::authoritative_contribution_entries_postgres(&mut tx, paper_id, &request.authors)
            .await?;
    let authoritative_contribution_hash = review_v4::contribution_ledger_hash(
        request.contribution_ledger_id,
        paper_id,
        &contribution_entries,
    )?;
    if request.contribution_ledger_hash != authoritative_contribution_hash {
        return Err(ApiError::conflict(
            "contribution_ledger_hash_mismatch",
            "release candidate must bind the complete authoritative contribution ledger",
        ));
    }
    let section_materialization_root = validated_revision_materialization_root(&revision)?;
    let candidate = PaperReleaseCandidateV2 {
        schema: PAPER_RELEASE_CANDIDATE_V2.to_string(),
        paper_project_id: paper.paper_project_id,
        revision_id,
        team_id: paper.team_id,
        challenge_id: paper.challenge_id,
        ruleset_hash,
        challenge_snapshot_hash,
        roster_version: team.roster_version,
        title: request.title.clone(),
        abstract_text: request.abstract_text.clone(),
        target_format: paper.target_format.clone(),
        source_manifest_hash: revision.source_manifest_hash.clone(),
        artifact_manifest_hash: revision.artifact_manifest_hash.clone(),
        bibliography_hash: revision.bibliography_hash.clone(),
        claim_evidence_graph_hash: revision.claim_evidence_graph_hash.clone(),
        section_materialization_root: Some(section_materialization_root),
        collaboration_compact_hash: request.collaboration_compact_hash.clone(),
        research_protocol_snapshot_hash: request.research_protocol_snapshot_hash.clone(),
        ethics_disclosure_hash: request.ethics_disclosure_hash.clone(),
        coi_disclosure_hash: request.coi_disclosure_hash.clone(),
        contribution_ledger_hash: request.contribution_ledger_hash.clone(),
        ai_disclosure_hash: request.ai_disclosure_hash.clone(),
        license: request.license.clone(),
        authors: request.authors.clone(),
    };
    let release_hash = paper_release_candidate_hash(&candidate)
        .map_err(|message| ApiError::bad_request("invalid_release_candidate", message))?;
    reserve_contribution_ledger_id_postgres(
        &mut tx,
        request.contribution_ledger_id,
        paper_id,
        &release_hash,
        now,
    )
    .await?;
    revision.status = PaperRevisionStatus::ReleaseCandidate;
    revision.release_candidate = Some(candidate);
    revision.release_candidate_hash = Some(release_hash.clone());
    revision.version += 1;
    revision.updated_at = now;
    let revision_json = serde_json::to_value(&revision)
        .map_err(|error| ApiError::internal(format!("encode paper revision: {error}")))?;
    let revision_updated = sqlx::query(
        "update hepta_paper_revisions
         set status = $1, release_candidate_hash = $2, version = $3,
             record_json = $4::jsonb, updated_at = $5
         where revision_id = $6 and paper_project_id = $7 and version = $8",
    )
    .bind(revision.status.as_str())
    .bind(&release_hash)
    .bind(revision.version as i64)
    .bind(revision_json)
    .bind(revision.updated_at)
    .bind(revision.revision_id)
    .bind(revision.paper_project_id)
    .bind(request.expected_revision_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if revision_updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "paper revision changed concurrently",
        ));
    }
    paper.release_candidate_revision_id = Some(revision_id);
    paper.version += 1;
    paper.updated_at = now;
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let paper_updated = sqlx::query(
        "update hepta_paper_projects
         set version = $1, record_json = $2::jsonb, updated_at = $3
         where paper_project_id = $4 and version = $5",
    )
    .bind(paper.version as i64)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_paper_version as i64)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if paper_updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "aggregate_version_conflict",
            "paper project changed concurrently",
        ));
    }
    let response = PromoteReleaseCandidateResponse {
        revision,
        release_candidate_hash: release_hash,
        paper_version: paper.version,
    };
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.release_candidate.promoted.v2",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "revision_id": revision_id,
            "release_candidate_hash": response.release_candidate_hash,
            "contribution_ledger_id": request.contribution_ledger_id,
            "roster_version": team.roster_version,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(revision_id),
        StatusCode::OK,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(response)))
}

fn verify_authorship_signature(
    public_key: &str,
    signing: &AuthorshipConsentSigningV2,
    signature: &str,
) -> Result<(), ApiError> {
    let verifying_key = crate::decode_verifying_key(public_key)?;
    let signature_bytes = BASE64.decode(signature).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "signature must be canonical padded base64",
        )
    })?;
    if BASE64.encode(&signature_bytes) != signature {
        return Err(ApiError::bad_request(
            "invalid_signature",
            "signature must be canonical padded base64",
        ));
    }
    let signature = Signature::from_slice(&signature_bytes).map_err(|_| {
        ApiError::bad_request(
            "invalid_signature",
            "signature must decode to 64 Ed25519 bytes",
        )
    })?;
    let signing_bytes = authorship_consent_signing_bytes(signing)
        .map_err(|message| ApiError::bad_request("invalid_authorship_consent", message))?;
    verifying_key
        .verify(&signing_bytes, &signature)
        .map_err(|_| {
            ApiError::forbidden(
                "authorship_signature_verification_failed",
                "authorship consent signature does not match the human player's key",
            )
        })
}

fn consent_signing_request(
    paper_id: Uuid,
    request: &CreateAuthorshipConsentRequest,
) -> AuthorshipConsentSigningV2 {
    AuthorshipConsentSigningV2 {
        schema: AUTHORSHIP_CONSENT_V2.to_string(),
        consent_id: request.consent_id,
        paper_project_id: paper_id,
        revision_id: request.revision_id,
        player_id: request.player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        release_candidate_hash: request.release_candidate_hash.clone(),
        signed_at_unix: request.signed_at_unix,
    }
}

async fn create_authorship_consent(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<CreateAuthorshipConsentRequest>,
) -> Result<(StatusCode, Json<AuthorshipConsent>), ApiError> {
    const OPERATION: &str = "create_authorship_consent_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("release_candidate_hash", &request.release_candidate_hash)?;
    validate_contract_text_api("signing_key_id", &request.signing_key_id)?;
    canonical_public_key(&request.signing_public_key)?;
    validate_digest_v2("signing_public_key_hash", &request.signing_public_key_hash)?;
    let signed_at = Utc
        .timestamp_opt(request.signed_at_unix, 0)
        .single()
        .ok_or_else(|| {
            ApiError::bad_request("invalid_signed_at", "signed_at_unix is out of range")
        })?;
    let now = Utc::now();
    if signed_at > now + chrono::Duration::minutes(5) {
        return Err(ApiError::bad_request(
            "invalid_signed_at",
            "authorship consent cannot be signed more than five minutes in the future",
        ));
    }
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/author-consents");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &canonical_path,
        &request.idempotency_key,
        &request_hash,
    )?;
    if assertion.player_id != request.player_id {
        return Err(ApiError::forbidden(
            "user_assertion_player_mismatch",
            "a player may only submit their own authorship consent",
        ));
    }
    let signing = consent_signing_request(paper_id, &request);

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        if memory.consents.contains_key(&request.consent_id) {
            return Err(ApiError::conflict(
                "authorship_consent_conflict",
                "consent_id already exists",
            ));
        }
        let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        // The signed timestamp proves authorship ordering, but the server's
        // receipt time is authoritative for the immutable gameplay deadline.
        ensure_paper_gameplay_active(&paper, now)?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        if paper.phase != PaperPhase::AuthorApproval
            || paper.release_candidate_revision_id != Some(request.revision_id)
        {
            return Err(ApiError::conflict(
                "paper_not_awaiting_author_approval",
                "paper must have this release candidate in author_approval",
            ));
        }
        let team = memory
            .teams
            .get(&paper.team_id)
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        if !team
            .members
            .iter()
            .any(|member| member.player_id == request.player_id)
        {
            return Err(ApiError::forbidden(
                "author_not_on_team",
                "only a member of the locked paper roster may sign",
            ));
        }
        let revision = memory.revisions.get(&request.revision_id).ok_or_else(|| {
            ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
        })?;
        if revision.paper_project_id != paper_id
            || revision.status != PaperRevisionStatus::ReleaseCandidate
            || revision.release_candidate_hash.as_deref()
                != Some(request.release_candidate_hash.as_str())
            || signed_at.timestamp() < revision.updated_at.timestamp()
        {
            return Err(ApiError::conflict(
                "release_candidate_mismatch",
                "consent must sign the current release candidate after its promotion",
            ));
        }
        if memory.consents.values().any(|consent| {
            consent.paper_project_id == paper_id
                && consent.revision_id == request.revision_id
                && consent.player_id == request.player_id
        }) {
            return Err(ApiError::conflict(
                "author_already_consented",
                "author already consented to this release candidate",
            ));
        }
        let player = memory
            .players
            .get(&request.player_id)
            .ok_or_else(|| ApiError::internal("author human player record is missing"))?;
        assert_player_identity(&assertion, player)?;
        if player.signing_key_id != request.signing_key_id
            || player.signing_public_key != request.signing_public_key
            || player.signing_public_key_hash != request.signing_public_key_hash
            || player.status != HumanPlayerStatus::Active
        {
            return Err(ApiError::conflict(
                "human_signing_key_snapshot_mismatch",
                "consent signing key snapshot does not match the registered player key",
            ));
        }
        let signing_key = memory
            .human_signing_keys
            .get(&(player.player_id, request.signing_key_id.clone()))
            .ok_or_else(|| ApiError::internal("current human signing key record is missing"))?;
        if signing_key.status != HumanSigningKeyStatus::Active
            || signing_key.retired_at.is_some()
            || signing_key.revoked_at.is_some()
            || signing_key.registered_at.timestamp() > signed_at.timestamp()
        {
            return Err(ApiError::forbidden(
                "authorship_key_inactive",
                "authorship consent requires the current active, unrevoked human key",
            ));
        }
        verify_authorship_signature(&player.signing_public_key, &signing, &request.signature)?;
        let consent = AuthorshipConsent {
            consent_id: request.consent_id,
            paper_project_id: paper_id,
            revision_id: request.revision_id,
            player_id: request.player_id,
            signing_key_id: request.signing_key_id.clone(),
            signing_public_key: request.signing_public_key.clone(),
            signing_public_key_hash: request.signing_public_key_hash.clone(),
            release_candidate_hash: request.release_candidate_hash.clone(),
            signed_at,
            signature: request.signature.clone(),
        };
        memory.consents.insert(consent.consent_id, consent.clone());
        let paper_version = {
            let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
            paper.version += 1;
            paper.updated_at = Utc::now();
            paper.version
        };
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.authorship_consent.accepted.v2",
            paper_id,
            paper_version,
            json!({
                "paper_project_id": paper_id,
                "revision_id": consent.revision_id,
                "player_id": consent.player_id,
                "release_candidate_hash": consent.release_candidate_hash,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &consent,
        )?;
        return Ok((StatusCode::CREATED, Json(consent)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    ensure_paper_gameplay_active(&paper, now)?;
    let paper_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if paper_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper_version,
        ));
    }
    if paper.phase != PaperPhase::AuthorApproval
        || paper.release_candidate_revision_id != Some(request.revision_id)
    {
        return Err(ApiError::conflict(
            "paper_not_awaiting_author_approval",
            "paper must have this release candidate in author_approval",
        ));
    }
    let member_exists = sqlx::query(
        "select 1 from hepta_research_team_members
         where team_id = $1 and player_id = $2",
    )
    .bind(paper.team_id)
    .bind(request.player_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .is_some();
    if !member_exists {
        return Err(ApiError::forbidden(
            "author_not_on_team",
            "only a member of the locked paper roster may sign",
        ));
    }
    let revision_row = sqlx::query(
        "select record_json from hepta_paper_revisions
         where revision_id = $1 and paper_project_id = $2 for share",
    )
    .bind(request.revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
    })?;
    let revision: PaperRevision = decode_record(revision_row.get("record_json"), "revision")?;
    if revision.status != PaperRevisionStatus::ReleaseCandidate
        || revision.release_candidate_hash.as_deref()
            != Some(request.release_candidate_hash.as_str())
        || signed_at.timestamp() < revision.updated_at.timestamp()
    {
        return Err(ApiError::conflict(
            "release_candidate_mismatch",
            "consent must sign the current release candidate after its promotion",
        ));
    }
    let player_row =
        sqlx::query("select record_json from hepta_human_players where player_id = $1 for share")
            .bind(request.player_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let player: HumanPlayer = decode_record(player_row.get("record_json"), "human player")?;
    assert_player_identity(&assertion, &player)?;
    if player.signing_key_id != request.signing_key_id
        || player.signing_public_key != request.signing_public_key
        || player.signing_public_key_hash != request.signing_public_key_hash
        || player.status != HumanPlayerStatus::Active
    {
        return Err(ApiError::conflict(
            "human_signing_key_snapshot_mismatch",
            "consent signing key snapshot does not match the registered player key",
        ));
    }
    let signing_key_row = sqlx::query(
        "select record_json from hepta_human_signing_keys
         where player_id = $1 and signing_key_id = $2 for share",
    )
    .bind(player.player_id)
    .bind(&request.signing_key_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| ApiError::internal("current human signing key record is missing"))?;
    let signing_key: HumanSigningKey =
        decode_record(signing_key_row.get("record_json"), "human signing key")?;
    if signing_key.status != HumanSigningKeyStatus::Active
        || signing_key.retired_at.is_some()
        || signing_key.revoked_at.is_some()
        || signing_key.registered_at.timestamp() > signed_at.timestamp()
    {
        return Err(ApiError::forbidden(
            "authorship_key_inactive",
            "authorship consent requires the current active, unrevoked human key",
        ));
    }
    verify_authorship_signature(&player.signing_public_key, &signing, &request.signature)?;
    let consent = AuthorshipConsent {
        consent_id: request.consent_id,
        paper_project_id: paper_id,
        revision_id: request.revision_id,
        player_id: request.player_id,
        signing_key_id: request.signing_key_id.clone(),
        signing_public_key: request.signing_public_key.clone(),
        signing_public_key_hash: request.signing_public_key_hash.clone(),
        release_candidate_hash: request.release_candidate_hash.clone(),
        signed_at,
        signature: request.signature.clone(),
    };
    let consent_json = serde_json::to_value(&consent)
        .map_err(|error| ApiError::internal(format!("encode authorship consent: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_authorship_consents (
            consent_id, paper_project_id, revision_id, player_id,
            signing_key_id, signing_public_key, signing_public_key_hash, release_candidate_hash,
            signature, record_json, signed_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11)",
    )
    .bind(consent.consent_id)
    .bind(consent.paper_project_id)
    .bind(consent.revision_id)
    .bind(consent.player_id)
    .bind(&consent.signing_key_id)
    .bind(&consent.signing_public_key)
    .bind(&consent.signing_public_key_hash)
    .bind(&consent.release_candidate_hash)
    .bind(&consent.signature)
    .bind(consent_json)
    .bind(consent.signed_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "authorship_consent_conflict",
                "consent_id or author consent already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    paper.version += 1;
    paper.updated_at = Utc::now();
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set version = $1, record_json = $2::jsonb, updated_at = $3
         where paper_project_id = $4 and version = $5",
    )
    .bind(paper.version as i64)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_paper_version as i64)
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
        "hepta.paper_raid.authorship_consent.accepted.v2",
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "revision_id": consent.revision_id,
            "player_id": consent.player_id,
            "release_candidate_hash": consent.release_candidate_hash,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(consent.consent_id),
        StatusCode::CREATED,
        &consent,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(consent)))
}

fn build_paper_bundle(
    team: &ResearchTeam,
    revision: &PaperRevision,
    consents: &[AuthorshipConsent],
) -> Result<PaperBundleV2, ApiError> {
    let candidate = revision.release_candidate.clone().ok_or_else(|| {
        ApiError::conflict(
            "release_candidate_missing",
            "paper revision does not contain a release candidate",
        )
    })?;
    let release_hash = revision.release_candidate_hash.clone().ok_or_else(|| {
        ApiError::conflict(
            "release_candidate_missing",
            "paper revision does not contain a release candidate hash",
        )
    })?;
    if candidate.team_id != team.team_id
        || candidate.roster_version != team.roster_version
        || candidate.authors.len() != team.members.len()
        || consents.len() != team.members.len()
    {
        return Err(ApiError::conflict(
            "paper_bundle_roster_mismatch",
            "release candidate and consent set must match the locked team roster",
        ));
    }
    let mut authors = candidate.authors.clone();
    authors.sort_by_key(|author| author.author_order);
    let mut bundle_consents = Vec::with_capacity(authors.len());
    let mut seen_consents = HashSet::new();
    for author in &authors {
        let member = team
            .members
            .iter()
            .find(|member| member.player_id == author.player_id)
            .ok_or_else(|| {
                ApiError::conflict(
                    "paper_bundle_roster_mismatch",
                    "release candidate author is not on the locked team",
                )
            })?;
        if member.participant_slot != author.participant_slot {
            return Err(ApiError::conflict(
                "paper_bundle_roster_mismatch",
                "release candidate author roster slot changed",
            ));
        }
        let consent = consents
            .iter()
            .find(|consent| consent.player_id == author.player_id)
            .ok_or_else(|| {
                ApiError::conflict(
                    "missing_authorship_consent",
                    format!("player {} has not signed", author.player_id),
                )
            })?;
        if !seen_consents.insert(consent.consent_id)
            || consent.paper_project_id != revision.paper_project_id
            || consent.revision_id != revision.revision_id
            || consent.release_candidate_hash != release_hash
        {
            return Err(ApiError::conflict(
                "authorship_consent_mismatch",
                "authorship consent set contains a duplicate or mismatched record",
            ));
        }
        let signing = AuthorshipConsentSigningV2 {
            schema: AUTHORSHIP_CONSENT_V2.to_string(),
            consent_id: consent.consent_id,
            paper_project_id: consent.paper_project_id,
            revision_id: consent.revision_id,
            player_id: consent.player_id,
            signing_key_id: consent.signing_key_id.clone(),
            signing_public_key: consent.signing_public_key.clone(),
            signing_public_key_hash: consent.signing_public_key_hash.clone(),
            release_candidate_hash: consent.release_candidate_hash.clone(),
            signed_at_unix: consent.signed_at.timestamp(),
        };
        verify_authorship_signature(&consent.signing_public_key, &signing, &consent.signature)?;
        bundle_consents.push(PaperBundleAuthorConsentV2 {
            author_order: author.author_order,
            participant_slot: author.participant_slot,
            player_id: author.player_id,
            consent_id: consent.consent_id,
            signing_key_id: consent.signing_key_id.clone(),
            signing_public_key: consent.signing_public_key.clone(),
            signing_public_key_hash: consent.signing_public_key_hash.clone(),
            signed_at_unix: consent.signed_at.timestamp(),
            signature: consent.signature.clone(),
        });
    }
    let mut bundle = PaperBundleV2 {
        schema: PAPER_BUNDLE_V2.to_string(),
        release_candidate: candidate,
        release_candidate_hash: release_hash,
        author_consents: bundle_consents,
        paper_bundle_hash: String::new(),
    };
    bundle.paper_bundle_hash = paper_bundle_hash(&bundle)
        .map_err(|message| ApiError::bad_request("invalid_paper_bundle", message))?;
    Ok(bundle)
}

fn authorship_key_integrity_hold(
    consents: &[AuthorshipConsent],
    signing_keys: &HashMap<(Uuid, String), HumanSigningKey>,
) -> Result<bool, ApiError> {
    let mut hold = false;
    for consent in consents {
        let key = signing_keys
            .get(&(consent.player_id, consent.signing_key_id.clone()))
            .ok_or_else(|| {
                ApiError::conflict(
                    "authorship_key_history_missing",
                    "authorship consent signing key is absent from durable key history",
                )
            })?;
        if key.signing_public_key != consent.signing_public_key
            || key.signing_public_key_hash != consent.signing_public_key_hash
            || key.registered_at.timestamp() > consent.signed_at.timestamp()
            || key
                .retired_at
                .is_some_and(|retired_at| retired_at.timestamp() < consent.signed_at.timestamp())
        {
            return Err(ApiError::conflict(
                "authorship_key_invalid_at_signing",
                "authorship consent key was not valid at its signed_at timestamp",
            ));
        }
        if key.status == HumanSigningKeyStatus::Revoked || key.revoked_at.is_some() {
            hold = true;
        }
    }
    Ok(hold)
}

async fn finalize_paper(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<FinalizePaperRequest>,
) -> Result<(StatusCode, Json<JointPaperSubmission>), ApiError> {
    const OPERATION: &str = "finalize_joint_paper_submission_v2";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_digest_v2("release_candidate_hash", &request.release_candidate_hash)?;
    let request_hash = request_hash(&request)?;
    let canonical_path = format!("/v2/hepta/papers/{paper_id}/finalize");
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
        let now = Utc::now();
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, paper_id).await?;
        if memory.submissions.contains_key(&request.submission_id)
            || memory.submissions.values().any(|submission| {
                submission.paper_project_id == paper_id
                    && submission.status == JointSubmissionStatus::SubmissionReady
            })
        {
            return Err(ApiError::conflict(
                "joint_submission_conflict",
                "submission_id or paper submission already exists",
            ));
        }
        let paper = memory.papers.get(&paper_id).cloned().ok_or_else(|| {
            ApiError::not_found("paper_project_not_found", "paper project does not exist")
        })?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        ensure_paper_gameplay_active(&paper, now)?;
        if paper.phase != PaperPhase::AuthorApproval
            || paper.current_revision_id != Some(request.revision_id)
            || paper.release_candidate_revision_id != Some(request.revision_id)
        {
            return Err(ApiError::conflict(
                "paper_not_finalizable",
                "paper must be in author_approval with the current release candidate",
            ));
        }
        let team = memory
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, &team, &assertion)?;
        if team.status != TeamStatus::Locked || team.challenge_id != paper.challenge_id {
            return Err(ApiError::conflict(
                "paper_team_invariant_failed",
                "paper team is no longer the locked challenge roster",
            ));
        }
        let revision = memory
            .revisions
            .get(&request.revision_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
            })?;
        if revision.paper_project_id != paper_id
            || revision.status != PaperRevisionStatus::ReleaseCandidate
            || revision.release_candidate_hash.as_deref()
                != Some(request.release_candidate_hash.as_str())
        {
            return Err(ApiError::conflict(
                "release_candidate_mismatch",
                "finalization release candidate does not match the paper project",
            ));
        }
        let consents: Vec<_> = memory
            .consents
            .values()
            .filter(|consent| {
                consent.paper_project_id == paper_id && consent.revision_id == request.revision_id
            })
            .cloned()
            .collect();
        let consented_players = consents
            .iter()
            .map(|consent| consent.player_id)
            .collect::<HashSet<_>>();
        let victory_facts = author_phase_gate_facts_memory(&memory, &paper);
        ensure_challenge_victory_gate(
            &paper,
            victory_facts,
            true,
            consented_players.len() == team.members.len(),
        )?;
        let signing_keys = consents
            .iter()
            .map(|consent| {
                let key = memory
                    .human_signing_keys
                    .get(&(consent.player_id, consent.signing_key_id.clone()))
                    .cloned()
                    .ok_or_else(|| {
                        ApiError::conflict(
                            "authorship_key_history_missing",
                            "authorship consent signing key is absent from durable key history",
                        )
                    })?;
                Ok(((consent.player_id, consent.signing_key_id.clone()), key))
            })
            .collect::<Result<HashMap<_, _>, ApiError>>()?;
        let integrity_hold = authorship_key_integrity_hold(&consents, &signing_keys)?;
        let bundle = build_paper_bundle(&team, &revision, &consents)?;
        let submission = JointPaperSubmission {
            submission_id: request.submission_id,
            paper_project_id: paper_id,
            revision_id: request.revision_id,
            release_candidate_hash: request.release_candidate_hash.clone(),
            paper_bundle_hash: bundle.paper_bundle_hash.clone(),
            status: if integrity_hold {
                JointSubmissionStatus::IntegrityHold
            } else {
                JointSubmissionStatus::SubmissionReady
            },
            paper_bundle: bundle,
            created_at: now,
        };
        let rework_resubmission =
            rework_v1::prepare_rework_resubmission_memory(&memory, &submission, now)?;
        let rework_cycle = rework_resubmission
            .as_ref()
            .and_then(|record| memory.reworks.get(&record.rework_id))
            .map(|record| record.rework_cycle);
        memory
            .submissions
            .insert(submission.submission_id, submission.clone());
        if let Some(record) = &rework_resubmission {
            memory
                .rework_resubmissions
                .insert(record.rework_id, record.clone());
        }
        let paper_version = {
            let paper = memory.papers.get_mut(&paper_id).expect("paper exists");
            paper.phase = if integrity_hold {
                PaperPhase::IntegrityHold
            } else {
                PaperPhase::SubmissionReady
            };
            if !integrity_hold {
                paper.outcome = PaperChallengeOutcomeV1::SubmissionReady;
                paper.outcome_reason = None;
                paper.terminal_at = Some(now);
                paper.active_rework_id = None;
                paper.active_rework_cycle = None;
                paper.rework_expires_at = None;
            }
            paper.version += 1;
            paper.updated_at = now;
            paper.version
        };
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            if integrity_hold {
                "hepta.paper_raid.joint_submission.integrity_held.v2"
            } else {
                "hepta.paper_raid.joint_submission.finalized.v2"
            },
            paper_id,
            paper_version,
            json!({
                "paper_project_id": paper_id,
                "submission_id": submission.submission_id,
                "revision_id": submission.revision_id,
                "paper_bundle_hash": submission.paper_bundle_hash,
                "release_candidate_hash": submission.release_candidate_hash,
                "author_count": submission.paper_bundle.author_consents.len(),
                "settlement_state": if integrity_hold { "integrity_hold" } else { "pending_finality" },
                "rework_id": rework_resubmission.as_ref().map(|record| record.rework_id),
                "rework_cycle": rework_cycle,
                "replacement_review_round": rework_resubmission.as_ref().map(|record| record.replacement_review_round),
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            if integrity_hold {
                StatusCode::ACCEPTED
            } else {
                StatusCode::CREATED
            },
            &submission,
        )?;
        return Ok((
            if integrity_hold {
                StatusCode::ACCEPTED
            } else {
                StatusCode::CREATED
            },
            Json(submission),
        ));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let now = collaboration_v3::postgres_transaction_now(&mut tx).await?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx, paper_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let mut paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let paper_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if paper_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper_version,
        ));
    }
    ensure_paper_gameplay_active(&paper, now)?;
    if paper.phase != PaperPhase::AuthorApproval
        || paper.current_revision_id != Some(request.revision_id)
        || paper.release_candidate_revision_id != Some(request.revision_id)
    {
        return Err(ApiError::conflict(
            "paper_not_finalizable",
            "paper must be in author_approval with the current release candidate",
        ));
    }
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id = $1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    assert_team_actor_postgres(&mut tx, team.team_id, &assertion).await?;
    if team.status != TeamStatus::Locked || team.challenge_id != paper.challenge_id {
        return Err(ApiError::conflict(
            "paper_team_invariant_failed",
            "paper team is no longer the locked challenge roster",
        ));
    }
    let revision_row = sqlx::query(
        "select record_json from hepta_paper_revisions
         where revision_id = $1 and paper_project_id = $2 for share",
    )
    .bind(request.revision_id)
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_revision_not_found", "paper revision does not exist")
    })?;
    let revision: PaperRevision = decode_record(revision_row.get("record_json"), "revision")?;
    if revision.status != PaperRevisionStatus::ReleaseCandidate
        || revision.release_candidate_hash.as_deref()
            != Some(request.release_candidate_hash.as_str())
    {
        return Err(ApiError::conflict(
            "release_candidate_mismatch",
            "finalization release candidate does not match the paper project",
        ));
    }
    let consent_rows = sqlx::query(
        "select record_json from hepta_authorship_consents
         where paper_project_id = $1 and revision_id = $2
         order by player_id for share",
    )
    .bind(paper_id)
    .bind(request.revision_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let consents: Vec<AuthorshipConsent> = consent_rows
        .into_iter()
        .map(|row| decode_record(row.get("record_json"), "authorship consent"))
        .collect::<Result<_, _>>()?;
    let consented_players = consents
        .iter()
        .map(|consent| consent.player_id)
        .collect::<HashSet<_>>();
    let victory_facts = author_phase_gate_facts_postgres(&mut tx, &paper).await?;
    ensure_challenge_victory_gate(
        &paper,
        victory_facts,
        true,
        consented_players.len() == team.members.len(),
    )?;
    let mut signing_keys = HashMap::new();
    for consent in &consents {
        let key_row = sqlx::query(
            "select record_json from hepta_human_signing_keys
             where player_id = $1 and signing_key_id = $2 for share",
        )
        .bind(consent.player_id)
        .bind(&consent.signing_key_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| {
            ApiError::conflict(
                "authorship_key_history_missing",
                "authorship consent signing key is absent from durable key history",
            )
        })?;
        let key: HumanSigningKey = decode_record(key_row.get("record_json"), "human signing key")?;
        signing_keys.insert((consent.player_id, consent.signing_key_id.clone()), key);
    }
    let integrity_hold = authorship_key_integrity_hold(&consents, &signing_keys)?;
    let bundle = build_paper_bundle(&team, &revision, &consents)?;
    let submission = JointPaperSubmission {
        submission_id: request.submission_id,
        paper_project_id: paper_id,
        revision_id: request.revision_id,
        release_candidate_hash: request.release_candidate_hash.clone(),
        paper_bundle_hash: bundle.paper_bundle_hash.clone(),
        status: if integrity_hold {
            JointSubmissionStatus::IntegrityHold
        } else {
            JointSubmissionStatus::SubmissionReady
        },
        paper_bundle: bundle,
        created_at: now,
    };
    let rework_resubmission =
        rework_v1::prepare_rework_resubmission_postgres(&mut tx, &submission, now).await?;
    let submission_json = serde_json::to_value(&submission)
        .map_err(|error| ApiError::internal(format!("encode joint submission: {error}")))?;
    let result = sqlx::query(
        "insert into hepta_joint_paper_submissions (
            submission_id, paper_project_id, revision_id, release_candidate_hash,
            paper_bundle_hash, status, record_json, created_at
         ) values ($1,$2,$3,$4,$5,$6,$7::jsonb,$8)",
    )
    .bind(submission.submission_id)
    .bind(submission.paper_project_id)
    .bind(submission.revision_id)
    .bind(&submission.release_candidate_hash)
    .bind(&submission.paper_bundle_hash)
    .bind(submission.status.as_str())
    .bind(submission_json)
    .bind(submission.created_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "joint_submission_conflict",
                "submission_id, paper project, or PaperBundle hash already exists",
            ));
        }
        return Err(ApiError::database(error));
    }
    if let Some(record) = &rework_resubmission {
        rework_v1::insert_rework_resubmission_postgres(&mut tx, record).await?;
    }
    paper.phase = if integrity_hold {
        PaperPhase::IntegrityHold
    } else {
        PaperPhase::SubmissionReady
    };
    if !integrity_hold {
        paper.outcome = PaperChallengeOutcomeV1::SubmissionReady;
        paper.outcome_reason = None;
        paper.terminal_at = Some(now);
        paper.active_rework_id = None;
        paper.active_rework_cycle = None;
        paper.rework_expires_at = None;
    }
    paper.version += 1;
    paper.updated_at = now;
    let paper_json = serde_json::to_value(&paper)
        .map_err(|error| ApiError::internal(format!("encode paper project: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set phase = $1, outcome = $2, outcome_reason = $3, terminal_at = $4,
             active_rework_id = $5, active_rework_cycle = $6, rework_expires_at = $7,
             version = $8, record_json = $9::jsonb, updated_at = $10
         where paper_project_id = $11 and version = $12",
    )
    .bind(paper.phase.as_str())
    .bind(paper.outcome.as_str())
    .bind(&paper.outcome_reason)
    .bind(paper.terminal_at)
    .bind(paper.active_rework_id)
    .bind(paper.active_rework_cycle.map(|cycle| cycle as i64))
    .bind(paper.rework_expires_at)
    .bind(paper.version as i64)
    .bind(paper_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(request.expected_paper_version as i64)
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
        if integrity_hold {
            "hepta.paper_raid.joint_submission.integrity_held.v2"
        } else {
            "hepta.paper_raid.joint_submission.finalized.v2"
        },
        paper.paper_project_id,
        paper.version,
        json!({
            "paper_project_id": paper.paper_project_id,
            "submission_id": submission.submission_id,
            "revision_id": submission.revision_id,
            "paper_bundle_hash": submission.paper_bundle_hash,
            "release_candidate_hash": submission.release_candidate_hash,
            "author_count": submission.paper_bundle.author_consents.len(),
            "settlement_state": if integrity_hold { "integrity_hold" } else { "pending_finality" },
            "rework_id": rework_resubmission.as_ref().map(|record| record.rework_id),
            "replacement_review_round": rework_resubmission.as_ref().map(|record| record.replacement_review_round),
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(submission.submission_id),
        if integrity_hold {
            StatusCode::ACCEPTED
        } else {
            StatusCode::CREATED
        },
        &submission,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((
        if integrity_hold {
            StatusCode::ACCEPTED
        } else {
            StatusCode::CREATED
        },
        Json(submission),
    ))
}

async fn get_joint_submission(
    State(state): State<AppState>,
    Path(paper_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<JointPaperSubmission>, ApiError> {
    let path = format!("/v2/hepta/papers/{paper_id}/submission");
    let assertion =
        require_member_read_assertion(&headers, &state, "get_joint_paper_submission_v2", &path)?;
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
        return memory
            .submissions
            .values()
            .filter(|submission| submission.paper_project_id == paper_id)
            .max_by_key(|submission| submission.created_at)
            .cloned()
            .map(Json)
            .ok_or_else(|| {
                ApiError::not_found(
                    "joint_submission_not_found",
                    "joint paper submission does not exist",
                )
            });
    }
    let pool = state.pool.as_ref().expect("checked PostgreSQL pool");
    let mut tx = pool.begin().await.map_err(ApiError::database)?;
    let paper_row =
        sqlx::query("select team_id from hepta_paper_projects where paper_project_id = $1")
            .bind(paper_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(ApiError::database)?
            .ok_or_else(|| {
                ApiError::not_found("paper_project_not_found", "paper project does not exist")
            })?;
    assert_team_actor_postgres(&mut tx, paper_row.get("team_id"), &assertion).await?;
    let row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id = $1 order by created_at desc limit 1",
    )
    .bind(paper_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "joint_submission_not_found",
            "joint paper submission does not exist",
        )
    })?;
    let submission = decode_record(row.get("record_json"), "joint paper submission")?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok(Json(submission))
}

pub(crate) fn validate_logical_session_id(value: &str) -> Result<(), ApiError> {
    let mut bytes = value.bytes();
    let first = bytes
        .next()
        .ok_or_else(|| ApiError::bad_request("invalid_session_id", "session_id is required"))?;
    if value.len() > 128
        || !first.is_ascii_alphanumeric()
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(ApiError::bad_request(
            "invalid_session_id",
            "session_id must match [A-Za-z0-9][A-Za-z0-9._:-]{0,127}",
        ));
    }
    Ok(())
}

struct ResearchSessionAuthorizationInputs<'a> {
    state: &'a AppState,
    session_id: &'a str,
    ttl_seconds: Option<u64>,
    session_roster_version: u64,
    supersedes_roster_version: Option<u64>,
    replaced_participant_slot: Option<u32>,
    paper: &'a PaperProject,
    team: &'a ResearchTeam,
    players: &'a HashMap<Uuid, HumanPlayer>,
    bindings: &'a HashMap<Uuid, AgentBinding>,
    challenge: &'a crate::ResearchChallenge,
}

fn build_research_session_authorization_set(
    inputs: ResearchSessionAuthorizationInputs<'_>,
) -> Result<ResearchSessionAuthorizationSetV1, ApiError> {
    let ResearchSessionAuthorizationInputs {
        state,
        session_id,
        ttl_seconds,
        session_roster_version,
        supersedes_roster_version,
        replaced_participant_slot,
        paper,
        team,
        players,
        bindings,
        challenge,
    } = inputs;
    ensure_paper_gameplay_active(paper, Utc::now())?;
    if paper.team_id != team.team_id
        || paper.challenge_id != team.challenge_id
        || challenge.challenge_id != team.challenge_id
        || team.status != TeamStatus::Locked
        || !(3..=5).contains(&team.members.len())
    {
        return Err(ApiError::conflict(
            "research_session_scope_mismatch",
            "paper, team, and challenge must share one locked 3-5 member roster",
        ));
    }
    if paper.phase == PaperPhase::SubmissionReady {
        return Err(ApiError::conflict(
            "paper_project_finalized",
            "a finalized paper project cannot start another research session",
        ));
    }
    if session_roster_version == 0 {
        return Err(ApiError::bad_request(
            "invalid_session_roster_version",
            "session roster version must be positive",
        ));
    }
    let ttl_seconds = ttl_seconds.unwrap_or(300).clamp(60, 900);
    let issued_at = Utc::now();
    let requested_expires_at = issued_at + chrono::Duration::seconds(ttl_seconds as i64);
    let active_deadline = if paper.active_rework_id.is_some() {
        paper.rework_expires_at
    } else {
        paper.grace_expires_at
    };
    let expires_at = active_deadline
        .map(|active_deadline| requested_expires_at.min(active_deadline))
        .unwrap_or(requested_expires_at);
    if expires_at <= issued_at {
        return Err(ApiError::conflict(
            if paper.active_rework_id.is_some() {
                "paper_rework_window_elapsed"
            } else {
                "paper_challenge_deadline_elapsed"
            },
            "research session authorization cannot extend beyond the active Paper deadline",
        ));
    }
    let mut roster_entries = Vec::with_capacity(team.members.len());
    let mut authorization_ids = Vec::with_capacity(team.members.len());
    for member in &team.members {
        let player = players
            .get(&member.player_id)
            .ok_or_else(|| ApiError::internal("research team human player record is missing"))?;
        let binding = bindings
            .get(&member.binding_id)
            .ok_or_else(|| ApiError::internal("research team Agent binding record is missing"))?;
        if binding.player_id != player.player_id
            || binding.agent_id != member.agent_id
            || binding.status != AgentBindingStatus::Active
        {
            return Err(ApiError::conflict(
                "research_session_binding_mismatch",
                "team Agent binding no longer matches its human player",
            ));
        }
        let public_key = canonical_public_key(&binding.agent_public_key)?;
        let public_key_bytes = BASE64
            .decode(&public_key)
            .map_err(|_| ApiError::internal("canonical Agent public key decode failed"))?;
        let key_hash = sha256_digest(&public_key_bytes);
        if key_hash != binding.agent_public_key_hash || binding.agent_key_id != key_hash {
            return Err(ApiError::conflict(
                "research_session_binding_key_mismatch",
                "stored Agent binding key material is not self-consistent",
            ));
        }
        let authorization_id = Uuid::new_v4();
        authorization_ids.push(authorization_id);
        roster_entries.push(ResearchSessionRosterMemberV1 {
            participant_slot: member.participant_slot,
            authorization_id: authorization_id.to_string(),
            subject_user_id: player.nakama_user_id.to_string(),
            agent_id: binding.agent_id.clone(),
            agent_did: if binding.agent_id.starts_with("did:") {
                binding.agent_id.clone()
            } else {
                format!("did:trnm:{}", binding.agent_id)
            },
            agent_key_id: binding.agent_key_id.clone(),
            agent_key_hash: key_hash,
            role: member.role.clone(),
        });
    }
    let roster_root = research_session_roster_root(
        session_id,
        &team.team_id.to_string(),
        &paper.paper_project_id.to_string(),
        session_roster_version,
        &roster_entries,
    )
    .map_err(|message| ApiError::bad_request("invalid_research_session_roster", message))?;
    let (ruleset_hash, challenge_snapshot_hash) = paper_challenge_binding_hashes(paper, challenge)?;
    let mut members = Vec::with_capacity(team.members.len());
    for ((team_member, roster_member), authorization_id) in team
        .members
        .iter()
        .zip(roster_entries.iter())
        .zip(authorization_ids)
    {
        let binding = bindings
            .get(&team_member.binding_id)
            .expect("Agent binding checked while building roster");
        let claim = ResearchSessionAuthorizationClaimV1 {
            schema: crate::paper_raid_contracts::RESEARCH_SESSION_AUTHORIZATION_V1.to_string(),
            authorization_id: authorization_id.to_string(),
            session_id: session_id.to_string(),
            team_id: team.team_id.to_string(),
            paper_project_id: paper.paper_project_id.to_string(),
            challenge_id: paper.challenge_id.to_string(),
            agent_id: binding.agent_id.clone(),
            agent_did: roster_member.agent_did.clone(),
            agent_key_id: roster_member.agent_key_id.clone(),
            agent_public_key: canonical_public_key(&binding.agent_public_key)?,
            subject_user_id: roster_member.subject_user_id.clone(),
            participant_slot: team_member.participant_slot,
            role: team_member.role.clone(),
            roster_version: session_roster_version,
            roster_root: roster_root.clone(),
            ruleset_hash: ruleset_hash.clone(),
            challenge_snapshot_hash: challenge_snapshot_hash.clone(),
            issued_at_unix: issued_at.timestamp(),
            expires_at_unix: expires_at.timestamp(),
        };
        let authorization = sign_research_session_authorization(
            claim,
            &state.security.nakama_authorization_issuer_key_id,
            &state.security.nakama_authorization_signing_key,
        )
        .map_err(|message| {
            ApiError::internal(format!("sign research session authorization: {message}"))
        })?;
        members.push(ResearchSessionAuthorizationMemberV1 {
            player_id: team_member.player_id,
            binding_id: team_member.binding_id,
            authorization,
            consumed_at: None,
        });
    }
    Ok(ResearchSessionAuthorizationSetV1 {
        schema: "hepta.paper_raid.research_session_authorization_set.v1".to_string(),
        authorization_set_id: Uuid::new_v4(),
        session_id: session_id.to_string(),
        team_id: team.team_id,
        paper_project_id: paper.paper_project_id,
        challenge_id: paper.challenge_id,
        team_roster_version: team.roster_version,
        roster_version: session_roster_version,
        roster_root,
        supersedes_roster_version,
        replaced_participant_slot,
        status: ResearchSessionAuthorizationSetStatus::Issued,
        members,
        version: 1,
        issued_at,
        expires_at,
        consumed_at: None,
    })
}

fn ensure_replacement_matches_previous_epoch(
    previous: &ResearchSessionAuthorizationSetV1,
    replacement: &ResearchSessionAuthorizationSetV1,
    disconnected_participant_slot: u32,
) -> Result<(), ApiError> {
    if previous.session_id != replacement.session_id
        || previous.team_id != replacement.team_id
        || previous.paper_project_id != replacement.paper_project_id
        || previous.challenge_id != replacement.challenge_id
        || previous.team_roster_version != replacement.team_roster_version
        || replacement.roster_version != previous.roster_version + 1
        || replacement.supersedes_roster_version != Some(previous.roster_version)
        || replacement.replaced_participant_slot != Some(disconnected_participant_slot)
        || previous.members.len() != replacement.members.len()
    {
        return Err(ApiError::conflict(
            "replacement_epoch_scope_mismatch",
            "replacement must preserve logical session, team roster, paper, and challenge identity",
        ));
    }

    let mut changed_agent_key_slots = Vec::new();
    for (old, new) in previous.members.iter().zip(&replacement.members) {
        let old_claim = &old.authorization.claim;
        let new_claim = &new.authorization.claim;
        if old.player_id != new.player_id
            || old.binding_id != new.binding_id
            || old_claim.participant_slot != new_claim.participant_slot
            || old_claim.subject_user_id != new_claim.subject_user_id
            || old_claim.agent_id != new_claim.agent_id
            || old_claim.agent_did != new_claim.agent_did
            || old_claim.role != new_claim.role
            || old_claim.ruleset_hash != new_claim.ruleset_hash
            || old_claim.challenge_snapshot_hash != new_claim.challenge_snapshot_hash
            || old_claim.authorization_id == new_claim.authorization_id
        {
            return Err(ApiError::conflict(
                "replacement_epoch_identity_changed",
                "replacement must preserve every human, binding, Agent, slot, and role while minting fresh authorization IDs",
            ));
        }
        let key_changed = old_claim.agent_key_id != new_claim.agent_key_id
            || old_claim.agent_public_key != new_claim.agent_public_key;
        if key_changed {
            changed_agent_key_slots.push(new_claim.participant_slot);
        }
    }
    if changed_agent_key_slots.len() != 1
        || changed_agent_key_slots.first().copied() != Some(disconnected_participant_slot)
    {
        return Err(ApiError::conflict(
            "replacement_epoch_agent_key_scope",
            "only the declared disconnected participant slot may rotate its Agent key",
        ));
    }
    Ok(())
}

async fn issue_research_session_authorization_set(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IssueResearchSessionAuthorizationSetRequest>,
) -> Result<(StatusCode, Json<ResearchSessionAuthorizationSetV1>), ApiError> {
    const OPERATION: &str = "issue_research_session_authorization_set_v1";
    validate_idempotency_key(&request.idempotency_key)?;
    validate_logical_session_id(&request.session_id)?;
    if request.expected_paper_version == 0 || request.expected_team_version == 0 {
        return Err(ApiError::bad_request(
            "invalid_expected_version",
            "expected paper and team versions must be positive",
        ));
    }
    let request_hash = request_hash(&request)?;
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        "/v2/hepta/research-session-authorizations",
        &request.idempotency_key,
        &request_hash,
    )?;
    let challenges = state
        .inspect(|league| Ok(league.challenges.clone()))
        .await?;

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, request.paper_project_id).await?;
        if memory
            .research_session_authorization_sets
            .values()
            .any(|set| set.session_id == request.session_id)
        {
            return Err(ApiError::conflict(
                "research_session_id_conflict",
                "session_id already has an authorization set",
            ));
        }
        let paper = memory
            .papers
            .get(&request.paper_project_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_project_not_found", "paper project does not exist")
            })?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, &team, &assertion)?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        if team.version != request.expected_team_version {
            return Err(version_conflict(
                "research team",
                request.expected_team_version,
                team.version,
            ));
        }
        if memory
            .research_session_authorization_sets
            .values()
            .any(|set| {
                set.paper_project_id == paper.paper_project_id
                    && matches!(
                        set.status,
                        ResearchSessionAuthorizationSetStatus::Issued
                            | ResearchSessionAuthorizationSetStatus::Consumed
                    )
            })
        {
            return Err(ApiError::conflict(
                "research_session_epoch_conflict",
                "paper project already has a live research session for this roster version",
            ));
        }
        let challenge = challenges.get(&paper.challenge_id).ok_or_else(|| {
            ApiError::not_found("challenge_not_found", "research challenge does not exist")
        })?;
        let mut players = HashMap::new();
        let mut bindings = HashMap::new();
        for member in &team.members {
            players.insert(
                member.player_id,
                memory
                    .players
                    .get(&member.player_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("team player is missing"))?,
            );
            bindings.insert(
                member.binding_id,
                memory
                    .bindings
                    .get(&member.binding_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("team Agent binding is missing"))?,
            );
        }
        let set = build_research_session_authorization_set(ResearchSessionAuthorizationInputs {
            state: &state,
            session_id: &request.session_id,
            ttl_seconds: request.ttl_seconds,
            session_roster_version: 1,
            supersedes_roster_version: None,
            replaced_participant_slot: None,
            paper: &paper,
            team: &team,
            players: &players,
            bindings: &bindings,
            challenge,
        })?;
        memory
            .research_session_authorization_sets
            .insert((set.session_id.clone(), set.roster_version), set.clone());
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.research_session_authorizations.issued.v1",
            paper.paper_project_id,
            paper.version,
            json!({
                "session_id": set.session_id,
                "team_id": set.team_id,
                "paper_project_id": set.paper_project_id,
                "roster_version": set.roster_version,
                "roster_root": set.roster_root,
                "authorization_count": set.members.len(),
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &set,
        )?;
        return Ok((StatusCode::CREATED, Json(set)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx,
        request.paper_project_id,
    )
    .await?;
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for share",
    )
    .bind(request.paper_project_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let paper_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if paper_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper_version,
        ));
    }
    let team_row = sqlx::query(
        "select version, record_json from hepta_research_teams where team_id = $1 for share",
    )
    .bind(paper.team_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    let team_version = u64::try_from(team_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("team version is invalid"))?;
    if team_version != request.expected_team_version {
        return Err(version_conflict(
            "research team",
            request.expected_team_version,
            team_version,
        ));
    }
    assert_team_actor_postgres(&mut tx, team.team_id, &assertion).await?;
    let challenge = challenges.get(&paper.challenge_id).ok_or_else(|| {
        ApiError::not_found("challenge_not_found", "research challenge does not exist")
    })?;
    let mut players = HashMap::new();
    let mut bindings = HashMap::new();
    for member in &team.members {
        let player_row = sqlx::query(
            "select record_json from hepta_human_players where player_id = $1 for share",
        )
        .bind(member.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        players.insert(
            member.player_id,
            decode_record(player_row.get("record_json"), "human player")?,
        );
        let binding_row = sqlx::query(
            "select record_json from hepta_agent_bindings where binding_id = $1 for share",
        )
        .bind(member.binding_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        bindings.insert(
            member.binding_id,
            decode_record(binding_row.get("record_json"), "Agent binding")?,
        );
    }
    let set = build_research_session_authorization_set(ResearchSessionAuthorizationInputs {
        state: &state,
        session_id: &request.session_id,
        ttl_seconds: request.ttl_seconds,
        session_roster_version: 1,
        supersedes_roster_version: None,
        replaced_participant_slot: None,
        paper: &paper,
        team: &team,
        players: &players,
        bindings: &bindings,
        challenge,
    })?;
    let set_json = serde_json::to_value(&set).map_err(|error| {
        ApiError::internal(format!(
            "encode research session authorization set: {error}"
        ))
    })?;
    let result = sqlx::query(
        "insert into hepta_research_session_authorization_sets (
            authorization_set_id, session_id, team_id, paper_project_id, challenge_id,
            team_roster_version, roster_version, roster_root,
            supersedes_roster_version, replaced_participant_slot,
            status, version, record_json, issued_at, expires_at, consumed_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::jsonb,$14,$15,null)",
    )
    .bind(set.authorization_set_id)
    .bind(&set.session_id)
    .bind(set.team_id)
    .bind(set.paper_project_id)
    .bind(set.challenge_id)
    .bind(set.team_roster_version as i64)
    .bind(set.roster_version as i64)
    .bind(&set.roster_root)
    .bind(set.supersedes_roster_version.map(|version| version as i64))
    .bind(set.replaced_participant_slot.map(|slot| slot as i32))
    .bind(set.status.as_str())
    .bind(set.version as i64)
    .bind(set_json)
    .bind(set.issued_at)
    .bind(set.expires_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "research_session_id_conflict",
                "session_id already has an authorization set",
            ));
        }
        return Err(ApiError::database(error));
    }
    for member in &set.members {
        let authorization_id = Uuid::parse_str(&member.authorization.claim.authorization_id)
            .map_err(|_| ApiError::internal("generated authorization_id is not UUID"))?;
        let record_json = serde_json::to_value(member).map_err(|error| {
            ApiError::internal(format!("encode research session authorization: {error}"))
        })?;
        sqlx::query(
            "insert into hepta_research_session_authorizations (
                authorization_id, authorization_set_id, session_id, roster_version,
                participant_slot, player_id, binding_id, agent_id, consumed_at, record_json
             ) values ($1,$2,$3,$4,$5,$6,$7,$8,null,$9::jsonb)",
        )
        .bind(authorization_id)
        .bind(set.authorization_set_id)
        .bind(&set.session_id)
        .bind(set.roster_version as i64)
        .bind(member.authorization.claim.participant_slot as i32)
        .bind(member.player_id)
        .bind(member.binding_id)
        .bind(&member.authorization.claim.agent_id)
        .bind(record_json)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.research_session_authorizations.issued.v1",
        paper.paper_project_id,
        paper.version,
        json!({
            "session_id": set.session_id,
            "team_id": set.team_id,
            "paper_project_id": set.paper_project_id,
            "roster_version": set.roster_version,
            "roster_root": set.roster_root,
            "authorization_count": set.members.len(),
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper.paper_project_id),
        StatusCode::CREATED,
        &set,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(set)))
}

async fn replace_research_session_authorization_set(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ReplaceResearchSessionAuthorizationSetRequest>,
) -> Result<(StatusCode, Json<ResearchSessionAuthorizationSetV1>), ApiError> {
    const OPERATION: &str = "replace_research_session_authorization_set_v1";
    validate_logical_session_id(&session_id)?;
    validate_idempotency_key(&request.idempotency_key)?;
    if request.previous_roster_version == 0
        || request.expected_paper_version == 0
        || request.expected_team_version == 0
        || !(1..=5).contains(&request.disconnected_participant_slot)
    {
        return Err(ApiError::bad_request(
            "invalid_replacement_epoch",
            "previous/session versions must be positive and disconnected slot must be 1-5",
        ));
    }
    let request_hash = request_hash(&request)?;
    let resource = format!("/v2/hepta/research-session-authorizations/{session_id}/replace");
    let assertion = require_user_assertion(
        &headers,
        &state,
        OPERATION,
        "POST",
        &resource,
        &request.idempotency_key,
        &request_hash,
    )?;
    let challenges = state
        .inspect(|league| Ok(league.challenges.clone()))
        .await?;

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        ensure_paper_finality_v2_source_unsealed_memory(&state, request.paper_project_id).await?;
        let latest_roster_version = memory
            .research_session_authorization_sets
            .values()
            .filter(|set| set.session_id == session_id)
            .map(|set| set.roster_version)
            .max()
            .ok_or_else(|| {
                ApiError::not_found(
                    "authorization_set_not_found",
                    "logical research session does not exist",
                )
            })?;
        if latest_roster_version != request.previous_roster_version {
            return Err(ApiError::conflict(
                "stale_replacement_epoch",
                format!(
                    "replacement expected latest epoch {}, actual latest epoch is {}",
                    request.previous_roster_version, latest_roster_version
                ),
            ));
        }
        let previous_key = (session_id.clone(), request.previous_roster_version);
        let previous = memory
            .research_session_authorization_sets
            .get(&previous_key)
            .cloned()
            .expect("latest authorization epoch exists");
        if !matches!(
            previous.status,
            ResearchSessionAuthorizationSetStatus::Issued
                | ResearchSessionAuthorizationSetStatus::Consumed
        ) {
            return Err(ApiError::conflict(
                "replacement_epoch_not_live",
                "only an issued or consumed latest epoch may be replaced",
            ));
        }
        let paper = memory
            .papers
            .get(&request.paper_project_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found("paper_project_not_found", "paper project does not exist")
            })?;
        let team = memory
            .teams
            .get(&paper.team_id)
            .cloned()
            .ok_or_else(|| ApiError::internal("paper research team record is missing"))?;
        assert_team_actor_memory(&memory, &team, &assertion)?;
        if paper.version != request.expected_paper_version {
            return Err(version_conflict(
                "paper project",
                request.expected_paper_version,
                paper.version,
            ));
        }
        if team.version != request.expected_team_version {
            return Err(version_conflict(
                "research team",
                request.expected_team_version,
                team.version,
            ));
        }
        if previous.paper_project_id != paper.paper_project_id
            || previous.team_id != team.team_id
            || previous.challenge_id != paper.challenge_id
            || previous.team_roster_version != team.roster_version
            || !team
                .members
                .iter()
                .any(|member| member.participant_slot == request.disconnected_participant_slot)
        {
            return Err(ApiError::conflict(
                "replacement_epoch_scope_mismatch",
                "replacement is not for the current locked team and declared participant slot",
            ));
        }
        let challenge = challenges.get(&paper.challenge_id).ok_or_else(|| {
            ApiError::not_found("challenge_not_found", "research challenge does not exist")
        })?;
        let mut players = HashMap::new();
        let mut bindings = HashMap::new();
        for member in &team.members {
            players.insert(
                member.player_id,
                memory
                    .players
                    .get(&member.player_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("team player is missing"))?,
            );
            bindings.insert(
                member.binding_id,
                memory
                    .bindings
                    .get(&member.binding_id)
                    .cloned()
                    .ok_or_else(|| ApiError::internal("team Agent binding is missing"))?,
            );
        }
        let replacement =
            build_research_session_authorization_set(ResearchSessionAuthorizationInputs {
                state: &state,
                session_id: &session_id,
                ttl_seconds: request.ttl_seconds,
                session_roster_version: request.previous_roster_version + 1,
                supersedes_roster_version: Some(request.previous_roster_version),
                replaced_participant_slot: Some(request.disconnected_participant_slot),
                paper: &paper,
                team: &team,
                players: &players,
                bindings: &bindings,
                challenge,
            })?;
        ensure_replacement_matches_previous_epoch(
            &previous,
            &replacement,
            request.disconnected_participant_slot,
        )?;
        let superseded = memory
            .research_session_authorization_sets
            .get_mut(&previous_key)
            .expect("previous epoch exists");
        superseded.status = ResearchSessionAuthorizationSetStatus::Superseded;
        superseded.version += 1;
        memory.research_session_authorization_sets.insert(
            (replacement.session_id.clone(), replacement.roster_version),
            replacement.clone(),
        );
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.research_session_authorizations.replaced.v1",
            paper.paper_project_id,
            replacement.roster_version,
            json!({
                "session_id": replacement.session_id,
                "team_id": replacement.team_id,
                "paper_project_id": replacement.paper_project_id,
                "team_roster_version": replacement.team_roster_version,
                "previous_session_roster_version": request.previous_roster_version,
                "session_roster_version": replacement.roster_version,
                "replaced_participant_slot": request.disconnected_participant_slot,
                "roster_root": replacement.roster_root,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &replacement,
        )?;
        return Ok((StatusCode::CREATED, Json(replacement)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx,
        request.paper_project_id,
    )
    .await?;
    let previous_row = sqlx::query(
        "select record_json from hepta_research_session_authorization_sets
         where session_id = $1 and roster_version = $2
           and roster_version = (
               select max(latest.roster_version)
               from hepta_research_session_authorization_sets latest
               where latest.session_id = $1
           )
         for update",
    )
    .bind(&session_id)
    .bind(request.previous_roster_version as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "stale_replacement_epoch",
            "previous_roster_version is not the latest logical-session epoch",
        )
    })?;
    let previous: ResearchSessionAuthorizationSetV1 = decode_record(
        previous_row.get("record_json"),
        "previous research session authorization set",
    )?;
    if !matches!(
        previous.status,
        ResearchSessionAuthorizationSetStatus::Issued
            | ResearchSessionAuthorizationSetStatus::Consumed
    ) {
        return Err(ApiError::conflict(
            "replacement_epoch_not_live",
            "only an issued or consumed latest epoch may be replaced",
        ));
    }
    let paper_row = sqlx::query(
        "select version, record_json from hepta_paper_projects
         where paper_project_id = $1 for share",
    )
    .bind(request.paper_project_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let paper_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("paper version is invalid"))?;
    if paper_version != request.expected_paper_version {
        return Err(version_conflict(
            "paper project",
            request.expected_paper_version,
            paper_version,
        ));
    }
    let team_row = sqlx::query(
        "select version, record_json from hepta_research_teams where team_id = $1 for share",
    )
    .bind(paper.team_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    let team: ResearchTeam = decode_record(team_row.get("record_json"), "research team")?;
    let team_version = u64::try_from(team_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("team version is invalid"))?;
    if team_version != request.expected_team_version {
        return Err(version_conflict(
            "research team",
            request.expected_team_version,
            team_version,
        ));
    }
    assert_team_actor_postgres(&mut tx, team.team_id, &assertion).await?;
    if previous.paper_project_id != paper.paper_project_id
        || previous.team_id != team.team_id
        || previous.challenge_id != paper.challenge_id
        || previous.team_roster_version != team.roster_version
        || !team
            .members
            .iter()
            .any(|member| member.participant_slot == request.disconnected_participant_slot)
    {
        return Err(ApiError::conflict(
            "replacement_epoch_scope_mismatch",
            "replacement is not for the current locked team and declared participant slot",
        ));
    }
    let challenge = challenges.get(&paper.challenge_id).ok_or_else(|| {
        ApiError::not_found("challenge_not_found", "research challenge does not exist")
    })?;
    let mut players = HashMap::new();
    let mut bindings = HashMap::new();
    for member in &team.members {
        let player_row = sqlx::query(
            "select record_json from hepta_human_players where player_id = $1 for share",
        )
        .bind(member.player_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        players.insert(
            member.player_id,
            decode_record(player_row.get("record_json"), "human player")?,
        );
        let binding_row = sqlx::query(
            "select record_json from hepta_agent_bindings where binding_id = $1 for share",
        )
        .bind(member.binding_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        bindings.insert(
            member.binding_id,
            decode_record(binding_row.get("record_json"), "Agent binding")?,
        );
    }
    let replacement =
        build_research_session_authorization_set(ResearchSessionAuthorizationInputs {
            state: &state,
            session_id: &session_id,
            ttl_seconds: request.ttl_seconds,
            session_roster_version: request.previous_roster_version + 1,
            supersedes_roster_version: Some(request.previous_roster_version),
            replaced_participant_slot: Some(request.disconnected_participant_slot),
            paper: &paper,
            team: &team,
            players: &players,
            bindings: &bindings,
            challenge,
        })?;
    ensure_replacement_matches_previous_epoch(
        &previous,
        &replacement,
        request.disconnected_participant_slot,
    )?;
    let superseded_record = {
        let mut value = previous.clone();
        value.status = ResearchSessionAuthorizationSetStatus::Superseded;
        value.version += 1;
        serde_json::to_value(value).map_err(|error| {
            ApiError::internal(format!("encode superseded authorization set: {error}"))
        })?
    };
    let superseded = sqlx::query(
        "update hepta_research_session_authorization_sets
         set status = 'superseded', version = version + 1, record_json = $1::jsonb
         where authorization_set_id = $2 and status in ('issued','consumed')",
    )
    .bind(superseded_record)
    .bind(previous.authorization_set_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if superseded.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "replacement_epoch_race",
            "previous authorization epoch changed concurrently",
        ));
    }
    let replacement_json = serde_json::to_value(&replacement).map_err(|error| {
        ApiError::internal(format!("encode replacement authorization set: {error}"))
    })?;
    sqlx::query(
        "insert into hepta_research_session_authorization_sets (
            authorization_set_id, session_id, team_id, paper_project_id, challenge_id,
            team_roster_version, roster_version, roster_root,
            supersedes_roster_version, replaced_participant_slot,
            status, version, record_json, issued_at, expires_at, consumed_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13::jsonb,$14,$15,null)",
    )
    .bind(replacement.authorization_set_id)
    .bind(&replacement.session_id)
    .bind(replacement.team_id)
    .bind(replacement.paper_project_id)
    .bind(replacement.challenge_id)
    .bind(replacement.team_roster_version as i64)
    .bind(replacement.roster_version as i64)
    .bind(&replacement.roster_root)
    .bind(
        replacement
            .supersedes_roster_version
            .map(|version| version as i64),
    )
    .bind(
        replacement
            .replaced_participant_slot
            .map(|slot| slot as i32),
    )
    .bind(replacement.status.as_str())
    .bind(replacement.version as i64)
    .bind(replacement_json)
    .bind(replacement.issued_at)
    .bind(replacement.expires_at)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    for member in &replacement.members {
        let authorization_id = Uuid::parse_str(&member.authorization.claim.authorization_id)
            .map_err(|_| ApiError::internal("generated authorization_id is not UUID"))?;
        let record_json = serde_json::to_value(member).map_err(|error| {
            ApiError::internal(format!("encode replacement authorization: {error}"))
        })?;
        sqlx::query(
            "insert into hepta_research_session_authorizations (
                authorization_id, authorization_set_id, session_id, roster_version,
                participant_slot, player_id, binding_id, agent_id, consumed_at, record_json
             ) values ($1,$2,$3,$4,$5,$6,$7,$8,null,$9::jsonb)",
        )
        .bind(authorization_id)
        .bind(replacement.authorization_set_id)
        .bind(&replacement.session_id)
        .bind(replacement.roster_version as i64)
        .bind(member.authorization.claim.participant_slot as i32)
        .bind(member.player_id)
        .bind(member.binding_id)
        .bind(&member.authorization.claim.agent_id)
        .bind(record_json)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.research_session_authorizations.replaced.v1",
        paper.paper_project_id,
        replacement.roster_version,
        json!({
            "session_id": replacement.session_id,
            "team_id": replacement.team_id,
            "paper_project_id": replacement.paper_project_id,
            "team_roster_version": replacement.team_roster_version,
            "previous_session_roster_version": request.previous_roster_version,
            "session_roster_version": replacement.roster_version,
            "replaced_participant_slot": request.disconnected_participant_slot,
            "roster_root": replacement.roster_root,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper.paper_project_id),
        StatusCode::CREATED,
        &replacement,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(replacement)))
}

fn validate_consumption_request(
    request: &ConsumeResearchSessionAuthorizationSetRequest,
) -> Result<DateTime<Utc>, ApiError> {
    if request.schema != "hepta.paper_raid.research_session_consumption.v1" {
        return Err(ApiError::bad_request(
            "unsupported_consumption_schema",
            "expected hepta.paper_raid.research_session_consumption.v1",
        ));
    }
    validate_logical_session_id(&request.session_id)?;
    validate_digest_v2("roster_root", &request.roster_root)?;
    validate_idempotency_key(&request.idempotency_key)?;
    if request.roster_version == 0 || !(3..=5).contains(&request.authorization_ids.len()) {
        return Err(ApiError::bad_request(
            "invalid_authorization_epoch",
            "consumption must include one positive 3-5 member roster epoch",
        ));
    }
    let unique: HashSet<_> = request.authorization_ids.iter().copied().collect();
    if unique.len() != request.authorization_ids.len() {
        return Err(ApiError::bad_request(
            "duplicate_authorization_id",
            "authorization_ids must be unique",
        ));
    }
    Utc.timestamp_opt(request.consumed_at_unix, 0)
        .single()
        .ok_or_else(|| {
            ApiError::bad_request("invalid_consumed_at", "consumed_at_unix is out of range")
        })
}

fn ensure_consumption_matches_set(
    request: &ConsumeResearchSessionAuthorizationSetRequest,
    set: &ResearchSessionAuthorizationSetV1,
    consumed_at: DateTime<Utc>,
) -> Result<(), ApiError> {
    let expected_ids: Vec<Uuid> = set
        .members
        .iter()
        .map(|member| {
            Uuid::parse_str(&member.authorization.claim.authorization_id)
                .map_err(|_| ApiError::internal("stored authorization_id is not UUID"))
        })
        .collect::<Result<_, _>>()?;
    if request.session_id != set.session_id
        || request.roster_version != set.roster_version
        || request.roster_root != set.roster_root
        || request.authorization_ids != expected_ids
    {
        return Err(ApiError::conflict(
            "authorization_epoch_mismatch",
            "Nakama must consume the complete ordered authorization set for one roster epoch",
        ));
    }
    if set.status != ResearchSessionAuthorizationSetStatus::Issued
        || set
            .members
            .iter()
            .any(|member| member.consumed_at.is_some())
    {
        return Err(ApiError::conflict(
            "authorization_set_already_consumed",
            "research session authorization set was already consumed",
        ));
    }
    let now = Utc::now();
    if now >= set.expires_at
        || consumed_at.timestamp() < set.issued_at.timestamp()
        || consumed_at.timestamp() >= set.expires_at.timestamp()
        || consumed_at > now + chrono::Duration::minutes(5)
    {
        return Err(ApiError::forbidden(
            "authorization_set_expired",
            "authorization set is expired or consumption time is invalid",
        ));
    }
    Ok(())
}

fn authorization_consumption_receipt(
    set: &ResearchSessionAuthorizationSetV1,
    consumed_at: DateTime<Utc>,
    issuer_key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<AuthorizationSetConsumptionReceiptV1, ApiError> {
    let authorization_ids = set
        .members
        .iter()
        .map(|member| {
            Uuid::parse_str(&member.authorization.claim.authorization_id)
                .map_err(|_| ApiError::internal("stored authorization_id is not UUID"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    sign_authorization_set_consumption_receipt(
        AuthorizationSetConsumptionReceiptV1 {
            schema: "hepta.paper_raid.authorization_set_consumption_receipt.v1".to_string(),
            session_id: set.session_id.clone(),
            team_id: set.team_id,
            paper_project_id: set.paper_project_id,
            challenge_id: set.challenge_id,
            session_roster_version: set.roster_version,
            roster_root: set.roster_root.clone(),
            authorization_ids,
            consumed_at_unix: consumed_at.timestamp(),
            issuer_key_id: issuer_key_id.to_string(),
            signature: String::new(),
        },
        signing_key,
    )
    .map_err(ApiError::internal)
}

async fn consume_research_session_authorization_set(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ConsumeResearchSessionAuthorizationSetRequest>,
) -> Result<(StatusCode, Json<AuthorizationSetConsumptionReceiptV1>), ApiError> {
    const OPERATION: &str = "consume_research_session_authorization_set_v1";
    require_service_token(
        &headers,
        NAKAMA_TOKEN_HEADER,
        &state.security.nakama_token,
        "nakama_auth_failed",
    )?;
    let consumed_at = validate_consumption_request(&request)?;
    let request_hash = request_hash(&request)?;

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let snapshot = memory
            .research_session_authorization_sets
            .get(&(request.session_id.clone(), request.roster_version))
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "authorization_set_not_found",
                    "research session authorization set does not exist",
                )
            })?;
        ensure_paper_finality_v2_source_unsealed_memory(&state, snapshot.paper_project_id).await?;
        ensure_consumption_matches_set(&request, &snapshot, consumed_at)?;
        let consumed_set = {
            let set = memory
                .research_session_authorization_sets
                .get_mut(&(request.session_id.clone(), request.roster_version))
                .expect("set exists");
            set.status = ResearchSessionAuthorizationSetStatus::Consumed;
            set.version += 1;
            set.consumed_at = Some(consumed_at);
            for member in &mut set.members {
                member.consumed_at = Some(consumed_at);
            }
            set.clone()
        };
        let response = authorization_consumption_receipt(
            &consumed_set,
            consumed_at,
            &state.security.nakama_authorization_issuer_key_id,
            &state.security.nakama_authorization_signing_key,
        )?;
        memory.research_session_consumption_receipts.insert(
            (response.session_id.clone(), response.session_roster_version),
            response.clone(),
        );
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.research_session_authorizations.consumed.v1",
            consumed_set.paper_project_id,
            consumed_set.version,
            json!({
                "session_id": consumed_set.session_id,
                "team_id": consumed_set.team_id,
                "paper_project_id": consumed_set.paper_project_id,
                "session_roster_version": consumed_set.roster_version,
                "roster_root": consumed_set.roster_root,
                "authorization_count": consumed_set.members.len(),
                "consumed_at": consumed_at,
                "receipt": response,
            }),
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::OK,
            &response,
        )?;
        return Ok((StatusCode::OK, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let paper_project_id = sqlx::query_scalar::<_, Uuid>(
        "select paper_project_id from hepta_research_session_authorization_sets
         where session_id = $1 and roster_version = $2",
    )
    .bind(&request.session_id)
    .bind(request.roster_version as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "research session authorization set does not exist",
        )
    })?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx,
        paper_project_id,
    )
    .await?;
    let row = sqlx::query(
        "select record_json from hepta_research_session_authorization_sets
         where session_id = $1 and roster_version = $2 for update",
    )
    .bind(&request.session_id)
    .bind(request.roster_version as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "research session authorization set does not exist",
        )
    })?;
    let mut set: ResearchSessionAuthorizationSetV1 =
        decode_record(row.get("record_json"), "research session authorization set")?;
    if set.paper_project_id != paper_project_id {
        return Err(ApiError::conflict(
            "authorization_set_paper_changed",
            "authorization set changed Paper scope while awaiting its source lock",
        ));
    }
    ensure_consumption_matches_set(&request, &set, consumed_at)?;
    set.status = ResearchSessionAuthorizationSetStatus::Consumed;
    set.version += 1;
    set.consumed_at = Some(consumed_at);
    for member in &mut set.members {
        member.consumed_at = Some(consumed_at);
    }
    let set_json = serde_json::to_value(&set).map_err(|error| {
        ApiError::internal(format!(
            "encode research session authorization set: {error}"
        ))
    })?;
    let updated_set = sqlx::query(
        "update hepta_research_session_authorization_sets
         set status = $1, version = $2, record_json = $3::jsonb, consumed_at = $4
         where authorization_set_id = $5 and status = 'issued'",
    )
    .bind(set.status.as_str())
    .bind(set.version as i64)
    .bind(set_json)
    .bind(consumed_at)
    .bind(set.authorization_set_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated_set.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "authorization_set_already_consumed",
            "research session authorization set was already consumed",
        ));
    }
    for member in &set.members {
        let authorization_id = Uuid::parse_str(&member.authorization.claim.authorization_id)
            .map_err(|_| ApiError::internal("stored authorization_id is not UUID"))?;
        let record_json = serde_json::to_value(member).map_err(|error| {
            ApiError::internal(format!("encode consumed authorization: {error}"))
        })?;
        let updated = sqlx::query(
            "update hepta_research_session_authorizations
             set consumed_at = $1, record_json = $2::jsonb
             where authorization_id = $3 and authorization_set_id = $4 and consumed_at is null",
        )
        .bind(consumed_at)
        .bind(record_json)
        .bind(authorization_id)
        .bind(set.authorization_set_id)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::database)?;
        if updated.rows_affected() != 1 {
            return Err(ApiError::conflict(
                "partial_authorization_consumption",
                "authorization set cannot be partially or repeatedly consumed",
            ));
        }
    }
    let response = authorization_consumption_receipt(
        &set,
        consumed_at,
        &state.security.nakama_authorization_issuer_key_id,
        &state.security.nakama_authorization_signing_key,
    )?;
    let receipt_json = serde_json::to_value(&response).map_err(|error| {
        ApiError::internal(format!("encode authorization consumption receipt: {error}"))
    })?;
    let receipt_hash = canonical_json_sha256(&response).map_err(|error| {
        ApiError::internal(format!("hash authorization consumption receipt: {error}"))
    })?;
    sqlx::query(
        "insert into hepta_research_session_consumption_receipts (
            authorization_set_id, session_id, roster_version, roster_root,
            receipt_hash, record_json, consumed_at
         ) values ($1,$2,$3,$4,$5,$6::jsonb,$7)",
    )
    .bind(set.authorization_set_id)
    .bind(&set.session_id)
    .bind(set.roster_version as i64)
    .bind(&set.roster_root)
    .bind(receipt_hash)
    .bind(receipt_json)
    .bind(consumed_at)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.research_session_authorizations.consumed.v1",
        set.paper_project_id,
        set.version,
        json!({
            "session_id": set.session_id,
            "team_id": set.team_id,
            "paper_project_id": set.paper_project_id,
            "session_roster_version": set.roster_version,
            "roster_root": set.roster_root,
            "authorization_count": set.members.len(),
            "consumed_at": consumed_at,
            "receipt": response,
        }),
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(set.paper_project_id),
        StatusCode::OK,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::OK, Json(response)))
}

fn ensure_completion_matches_set(
    completion: &ResearchSessionCompletionV1,
    set: &ResearchSessionAuthorizationSetV1,
) -> Result<(Uuid, Uuid, Uuid), ApiError> {
    if set.status != ResearchSessionAuthorizationSetStatus::Consumed
        || set.consumed_at.is_none()
        || set
            .members
            .iter()
            .any(|member| member.consumed_at.is_none())
    {
        return Err(ApiError::conflict(
            "authorization_set_not_consumed",
            "completion requires an atomically consumed authorization epoch",
        ));
    }
    let team_id = Uuid::parse_str(&completion.team_id).map_err(|_| {
        ApiError::bad_request(
            "invalid_completion_identity",
            "completion team_id must be UUID",
        )
    })?;
    let paper_project_id = Uuid::parse_str(&completion.paper_project_id).map_err(|_| {
        ApiError::bad_request(
            "invalid_completion_identity",
            "completion paper_project_id must be UUID",
        )
    })?;
    let challenge_id = Uuid::parse_str(&completion.challenge_id).map_err(|_| {
        ApiError::bad_request(
            "invalid_completion_identity",
            "completion challenge_id must be UUID",
        )
    })?;
    let first_claim = set
        .members
        .first()
        .ok_or_else(|| ApiError::internal("authorization set has no members"))?
        .authorization
        .claim
        .clone();
    if completion.session_id != set.session_id
        || team_id != set.team_id
        || paper_project_id != set.paper_project_id
        || challenge_id != set.challenge_id
        || completion.roster_version != set.roster_version
        || completion.roster_root != set.roster_root
        || completion.ruleset_hash != first_claim.ruleset_hash
        || completion.challenge_snapshot_hash != first_claim.challenge_snapshot_hash
    {
        return Err(ApiError::conflict(
            "completion_authorization_epoch_mismatch",
            "completion identity or roots differ from the consumed authorization epoch",
        ));
    }
    if completion.terminal_facts.result_code != "paper_bundle_ready" {
        return Err(ApiError::conflict(
            "unsupported_completion_result",
            "Paper Raid v0 only accepts paper_bundle_ready completion facts",
        ));
    }
    Ok((team_id, paper_project_id, challenge_id))
}

fn ensure_completion_matches_submission(
    completion: &ResearchSessionCompletionV1,
    submission: &JointPaperSubmission,
) -> Result<(), ApiError> {
    if submission.status != JointSubmissionStatus::SubmissionReady
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
            "completion_paper_bundle_mismatch",
            "terminal facts do not bind the finalized canonical PaperBundle",
        ));
    }
    Ok(())
}

fn completion_receipt(
    completion: &ResearchSessionCompletionV1,
    team_id: Uuid,
    paper_project_id: Uuid,
    challenge_id: Uuid,
    verified_at: DateTime<Utc>,
    issuer_key_id: &str,
    signing_key: &ed25519_dalek::SigningKey,
) -> Result<NakamaResearchSessionCompletionReceiptV1, ApiError> {
    sign_nakama_completion_receipt(
        NakamaResearchSessionCompletionReceiptV1 {
            schema: "hepta.paper_raid.nakama_completion_receipt.v1".to_string(),
            commitment_id: completion.commitment_id.clone(),
            session_id: completion.session_id.clone(),
            team_id,
            paper_project_id,
            challenge_id,
            roster_version: completion.roster_version,
            roster_root: completion.roster_root.clone(),
            event_count: completion.event_count,
            event_root: completion.event_root.clone(),
            archive_hash: completion.archive_hash.clone(),
            ruleset_hash: completion.ruleset_hash.clone(),
            challenge_snapshot_hash: completion.challenge_snapshot_hash.clone(),
            nakama_authority_key_id: completion.authority_key_id.clone(),
            terminal_facts: completion.terminal_facts.clone(),
            verified_at_unix: verified_at.timestamp(),
            issuer_key_id: issuer_key_id.to_string(),
            signature: String::new(),
        },
        signing_key,
    )
    .map_err(ApiError::internal)
}

async fn ingest_nakama_research_session_completion(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<IngestNakamaResearchSessionCompletionRequestV1>,
) -> Result<(StatusCode, Json<NakamaResearchSessionCompletionReceiptV1>), ApiError> {
    const OPERATION: &str = "ingest_nakama_research_session_completion_v1";
    require_service_token(
        &headers,
        NAKAMA_TOKEN_HEADER,
        &state.security.nakama_token,
        "nakama_auth_failed",
    )?;
    if request.schema != "hepta.paper_raid.nakama_completion_ingest.v1" {
        return Err(ApiError::bad_request(
            "unsupported_completion_ingest_schema",
            "expected hepta.paper_raid.nakama_completion_ingest.v1",
        ));
    }
    validate_idempotency_key(&request.idempotency_key)?;
    let request_hash = request_hash(&request)?;
    state
        .security
        .verify_nakama_research_completion(&request.completion, &request.archive)
        .map_err(|message| ApiError::forbidden("invalid_nakama_completion", message))?;
    let verified_at = Utc::now();

    if state.pool.is_none() {
        let mut memory = state.paper_raid.write().await;
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        if memory.research_session_completions.contains_key(&(
            request.completion.session_id.clone(),
            request.completion.roster_version,
        )) {
            return Err(ApiError::conflict(
                "research_session_completion_conflict",
                "research session already has a completion receipt",
            ));
        }
        let set_snapshot = memory
            .research_session_authorization_sets
            .get(&(
                request.completion.session_id.clone(),
                request.completion.roster_version,
            ))
            .cloned()
            .ok_or_else(|| {
                ApiError::not_found(
                    "authorization_set_not_found",
                    "completion has no corresponding authorization set",
                )
            })?;
        ensure_paper_finality_v2_source_unsealed_memory(&state, set_snapshot.paper_project_id)
            .await?;
        let latest_roster_version = memory
            .research_session_authorization_sets
            .values()
            .filter(|set| set.session_id == request.completion.session_id)
            .map(|set| set.roster_version)
            .max()
            .ok_or_else(|| ApiError::internal("authorization epoch history is empty"))?;
        if latest_roster_version != request.completion.roster_version {
            return Err(ApiError::conflict(
                "completion_not_latest_authorization_epoch",
                "only the latest authorization epoch of a logical session may complete",
            ));
        }
        let (team_id, paper_project_id, challenge_id) =
            ensure_completion_matches_set(&request.completion, &set_snapshot)?;
        let submission = memory
            .submissions
            .values()
            .find(|submission| {
                submission.paper_project_id == paper_project_id
                    && submission.status == JointSubmissionStatus::SubmissionReady
            })
            .ok_or_else(|| {
                ApiError::conflict(
                    "paper_bundle_not_finalized",
                    "completion requires a finalized canonical PaperBundle",
                )
            })?;
        ensure_completion_matches_submission(&request.completion, submission)?;
        let response = completion_receipt(
            &request.completion,
            team_id,
            paper_project_id,
            challenge_id,
            verified_at,
            &state.security.nakama_authorization_issuer_key_id,
            &state.security.nakama_authorization_signing_key,
        )?;
        let set = memory
            .research_session_authorization_sets
            .get_mut(&(
                request.completion.session_id.clone(),
                request.completion.roster_version,
            ))
            .expect("set exists");
        set.status = ResearchSessionAuthorizationSetStatus::Completed;
        set.version += 1;
        memory.research_session_completions.insert(
            (response.session_id.clone(), response.roster_version),
            response.clone(),
        );
        push_memory_event(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.nakama_completion.verified.v1",
            paper_project_id,
            set_snapshot.version + 1,
            serde_json::to_value(&response)
                .map_err(|error| ApiError::internal(format!("encode completion event: {error}")))?,
        )?;
        memory_remember(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            request_hash,
            StatusCode::CREATED,
            &response,
        )?;
        return Ok((StatusCode::CREATED, Json(response)));
    }

    let (mut tx, replay) =
        begin_postgres_idempotent(&state, OPERATION, &request.idempotency_key, &request_hash)
            .await?;
    if let Some(replay) = replay {
        return decode_stored(replay);
    }
    let paper_project_id = sqlx::query_scalar::<_, Uuid>(
        "select paper_project_id from hepta_research_session_authorization_sets
         where session_id = $1 and roster_version = $2
           and roster_version = (
               select max(latest.roster_version)
               from hepta_research_session_authorization_sets latest
               where latest.session_id = $1
           )",
    )
    .bind(&request.completion.session_id)
    .bind(request.completion.roster_version as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "completion has no corresponding authorization set",
        )
    })?;
    crate::paper_chain_finality_v2::lock_paper_finality_v2_source_unsealed_postgres(
        &mut tx,
        paper_project_id,
    )
    .await?;
    let set_row = sqlx::query(
        "select record_json from hepta_research_session_authorization_sets
         where session_id = $1 and roster_version = $2
           and roster_version = (
               select max(latest.roster_version)
               from hepta_research_session_authorization_sets latest
               where latest.session_id = $1
           )
         for update",
    )
    .bind(&request.completion.session_id)
    .bind(request.completion.roster_version as i64)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found(
            "authorization_set_not_found",
            "completion has no corresponding authorization set",
        )
    })?;
    let mut set: ResearchSessionAuthorizationSetV1 = decode_record(
        set_row.get("record_json"),
        "research session authorization set",
    )?;
    if set.paper_project_id != paper_project_id {
        return Err(ApiError::conflict(
            "authorization_set_paper_changed",
            "authorization set changed Paper scope while awaiting its source lock",
        ));
    }
    let (team_id, completion_paper_project_id, challenge_id) =
        ensure_completion_matches_set(&request.completion, &set)?;
    debug_assert_eq!(completion_paper_project_id, paper_project_id);
    let submission_row = sqlx::query(
        "select record_json from hepta_joint_paper_submissions
         where paper_project_id = $1 and status = 'submission_ready' for share",
    )
    .bind(paper_project_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::conflict(
            "paper_bundle_not_finalized",
            "completion requires a finalized canonical PaperBundle",
        )
    })?;
    let submission: JointPaperSubmission =
        decode_record(submission_row.get("record_json"), "joint paper submission")?;
    ensure_completion_matches_submission(&request.completion, &submission)?;
    let response = completion_receipt(
        &request.completion,
        team_id,
        paper_project_id,
        challenge_id,
        verified_at,
        &state.security.nakama_authorization_issuer_key_id,
        &state.security.nakama_authorization_signing_key,
    )?;
    let record_json = serde_json::to_value(&response)
        .map_err(|error| ApiError::internal(format!("encode completion receipt: {error}")))?;
    let inserted = sqlx::query(
        "insert into hepta_nakama_research_session_completions (
            commitment_id, authorization_set_id, session_id,
            team_id, paper_project_id, challenge_id,
            roster_version, roster_root, event_count, event_root, archive_hash,
            ruleset_hash, challenge_snapshot_hash, authority_key_id,
            record_json, verified_at
         ) values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15::jsonb,$16)",
    )
    .bind(&response.commitment_id)
    .bind(set.authorization_set_id)
    .bind(&response.session_id)
    .bind(response.team_id)
    .bind(response.paper_project_id)
    .bind(response.challenge_id)
    .bind(response.roster_version as i64)
    .bind(&response.roster_root)
    .bind(response.event_count as i64)
    .bind(&response.event_root)
    .bind(&response.archive_hash)
    .bind(&response.ruleset_hash)
    .bind(&response.challenge_snapshot_hash)
    .bind(&response.nakama_authority_key_id)
    .bind(record_json)
    .bind(
        Utc.timestamp_opt(response.verified_at_unix, 0)
            .single()
            .expect("verified completion time is valid"),
    )
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation())
        {
            return Err(ApiError::conflict(
                "research_session_completion_conflict",
                "research session already has a completion receipt",
            ));
        }
        return Err(ApiError::database(error));
    }
    set.status = ResearchSessionAuthorizationSetStatus::Completed;
    set.version += 1;
    let set_json = serde_json::to_value(&set).map_err(|error| {
        ApiError::internal(format!("encode completed authorization set: {error}"))
    })?;
    let updated = sqlx::query(
        "update hepta_research_session_authorization_sets
         set status = 'completed', version = $1, record_json = $2::jsonb
         where authorization_set_id = $3 and status = 'consumed'",
    )
    .bind(set.version as i64)
    .bind(set_json)
    .bind(set.authorization_set_id)
    .execute(&mut *tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "authorization_set_not_consumed",
            "completion cannot transition an unconsumed or completed authorization epoch",
        ));
    }
    insert_postgres_event(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.nakama_completion.verified.v1",
        paper_project_id,
        set.version,
        serde_json::to_value(&response)
            .map_err(|error| ApiError::internal(format!("encode completion event: {error}")))?,
    )
    .await?;
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(paper_project_id),
        StatusCode::CREATED,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[cfg(test)]
#[path = "paper_raid_v2_tests.rs"]
pub(crate) mod endpoint_tests;
