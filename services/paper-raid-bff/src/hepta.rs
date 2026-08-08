use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use hepta_paper_raid_contracts::{
    authorship_consent_signing_bytes, canonical_json_bytes, human_key_registration_signing_bytes,
    paper_appeal_signing_bytes, section_review_signing_bytes, sha256_digest,
    sign_consumer_user_assertion, team_member_acceptance_signing_bytes,
    verify_human_key_registration_pop, AuthorshipConsentSigningV2, ConsumerUserAssertionClaimV2,
    HumanKeyRegistrationClaimV2, PaperAppealSigningV1, SectionReviewSigningV1,
    TeamMemberAcceptanceSigningV2, AUTHORSHIP_CONSENT_V2, CONSUMER_USER_ASSERTION_V2,
    HUMAN_KEY_REGISTRATION_V2, PAPER_APPEAL_V1, SECTION_REVIEW_V1, TEAM_MEMBER_ACCEPTANCE_V2,
};
use reqwest::{header, Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use url::Url;
use uuid::Uuid;

use crate::{
    config::{AlphaIdentity, ConsumerAssertionConfig},
    error::AppError,
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
    TransitionPaperProject,
    CreatePaperWorkItem,
    TransitionPaperWorkItem,
    CreatePaperRevision,
    PromotePaperReleaseCandidate,
    CreateAuthorshipConsent,
    FinalizeJointPaperSubmission,
    IssueResearchSessionAuthorizationSet,
    ReplaceResearchSessionAuthorizationSet,
    CreateNakamaResearchSessionControl,
    ResumeNakamaResearchSessionControl,
    ReplaceNakamaResearchSessionRosterControl,
    CompleteNakamaResearchSessionControl,
    QueueMatchmaking,
    DecideTeamProposal,
    MaterializeTeamProposal,
    CreateEvidenceCard,
    CreateCitationRecord,
    CreateExperimentPlan,
    CreateRunRecord,
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
    CreatePaperEvaluation,
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
    release_candidate_hash: String,
    grounds_hash: String,
    evidence_manifest_hash: String,
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
            CommandName::CreatePaperEvaluation => post(
                format!("/v2/hepta/papers/{}/evaluations", resource()?),
                "create_paper_evaluation_v1",
            ),
            CommandName::SubmitReproduction => post(
                format!(
                    "/v2/hepta/papers/{}/evaluations/{}/reproductions",
                    resource()?,
                    child()?
                ),
                "create_paper_reproduction_v1",
            ),
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
}

impl HeptaClient {
    pub fn new(
        base: Url,
        assertions: ConsumerAssertionConfig,
        pool: PgPool,
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
            CommandName::SubmitReview => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let section_revision_id = request
                    .child_id
                    .ok_or_else(|| AppError::Invalid("child_id is required".into()))?;
                let input: ReviewFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| AppError::Invalid("invalid review frame payload".into()))?;
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
            CommandName::SubmitAppeal => {
                let paper_id = request
                    .resource_id
                    .ok_or_else(|| AppError::Invalid("resource_id is required".into()))?;
                let evaluation_id = request
                    .child_id
                    .ok_or_else(|| AppError::Invalid("child_id is required".into()))?;
                let input: AppealFramePayload = serde_json::from_value(request.payload.clone())
                    .map_err(|_| AppError::Invalid("invalid Appeal frame payload".into()))?;
                let review = self.get_paper_review_state(identity, paper_id).await?;
                let evaluation_id_text = evaluation_id.to_string();
                let evaluation = review
                    .get("evaluations")
                    .and_then(Value::as_array)
                    .and_then(|evaluations| {
                        evaluations.iter().find(|evaluation| {
                            evaluation.get("evaluation_id").and_then(Value::as_str)
                                == Some(evaluation_id_text.as_str())
                        })
                    })
                    .ok_or(AppError::NotFound)?;
                let current_release = evaluation
                    .get("release_candidate_hash")
                    .and_then(Value::as_str)
                    .ok_or(AppError::Upstream)?;
                let paper_id_text = paper_id.to_string();
                if evaluation.get("paper_project_id").and_then(Value::as_str)
                    != Some(paper_id_text.as_str())
                    || current_release != input.release_candidate_hash
                {
                    return Err(AppError::Conflict(
                        "Appeal payload does not match the evaluated release candidate".into(),
                    ));
                }
                let signing = PaperAppealSigningV1 {
                    schema: PAPER_APPEAL_V1.into(),
                    appeal_id: input.appeal_id,
                    evaluation_id,
                    paper_project_id: paper_id,
                    release_candidate_hash: input.release_candidate_hash.clone(),
                    appellant_player_id: identity.player_id,
                    grounds_hash: input.grounds_hash.clone(),
                    evidence_manifest_hash: input.evidence_manifest_hash.clone(),
                    signing_key_id: player.signing_key_id.clone(),
                    signing_public_key_hash: player.signing_public_key_hash.clone(),
                    signed_at_unix,
                };
                let bytes = paper_appeal_signing_bytes(&signing).map_err(AppError::Invalid)?;
                let payload = serde_json::json!({
                    "appeal_id": input.appeal_id,
                    "release_candidate_hash": input.release_candidate_hash,
                    "appellant_player_id": identity.player_id,
                    "grounds_hash": input.grounds_hash,
                    "evidence_manifest_hash": input.evidence_manifest_hash,
                    "signing_key_id": player.signing_key_id.clone(),
                    "signing_public_key": player.signing_public_key.clone(),
                    "signing_public_key_hash": player.signing_public_key_hash.clone(),
                    "signed_at_unix": signed_at_unix,
                });
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

    pub async fn get_paper_room(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/papers/{paper_id}/room"),
            None,
            "get_paper_room_v3",
        )
        .await
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

    pub async fn get_paper_review_state(
        &self,
        identity: &AlphaIdentity,
        paper_id: Uuid,
    ) -> Result<Value, AppError> {
        self.get_json(
            identity,
            format!("/v2/hepta/papers/{paper_id}/review-state"),
            None,
            "get_paper_review_state_v1",
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
        let response = request
            .send()
            .await
            .map_err(|_| AppError::Unavailable("hepta"))?;
        if response.status().is_redirection() || !strict_json(response.headers()) {
            return Err(AppError::Upstream);
        }
        let status = response.status();
        let bytes = limited_body(response, MAX_JSON_BYTES).await?;
        let _: Value = serde_json::from_slice(&bytes).map_err(|_| AppError::Upstream)?;
        if status.is_server_error() {
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
    fn p5_child_routes_fail_closed_without_exact_child() {
        let command = BrowserCommand {
            command: CommandName::SubmitReproduction,
            resource_id: Some(Uuid::new_v4()),
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: Value::Null,
        };
        assert!(matches!(command.route(), Err(AppError::Invalid(_))));
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
                command(CommandName::CreatePaperEvaluation, Some(paper), None, None),
                format!("/v2/hepta/papers/{paper}/evaluations"),
                "create_paper_evaluation_v1",
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

        let first_process =
            HeptaClient::new(base.clone(), assertions.clone(), pool.clone()).expect("first client");
        assert!(first_process
            .begin_idempotent(&identity, request.idempotency_key, &request_hash)
            .await
            .expect("seed pending request")
            .is_none());
        drop(first_process);

        let restarted = HeptaClient::new(base, assertions, pool).expect("restarted client");
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
