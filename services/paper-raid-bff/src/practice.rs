use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const PRACTICE_UNRANKED_MODE: &str = "practice_unranked";
pub const PRACTICE_SCENARIO_EVIDENCE_AUDIT_INTRO_V1: &str = "evidence-audit-intro-v1";
pub const PRACTICE_SESSION_V1: &str = "hepta.paper_raid.practice_session.v1";
pub const PRACTICE_ELIGIBILITY_V1: &str = "hepta.paper_raid.practice_eligibility.v1";
pub const PRACTICE_BROWSER_ACTION_V1: &str = "hepta.paper_raid.practice_browser_action.v1";
pub const PRACTICE_BRIDGE_CLAIM_V1: &str = "hepta.paper_raid.practice_bridge_claim.v1";
pub const PRACTICE_BRIDGE_RESULT_V1: &str = "hepta.paper_raid.practice_bridge_result.v1";
pub const PRACTICE_EVENT_V1: &str = "hepta.paper_raid.practice_event.v1";
pub const PRACTICE_AUTHORITY_NONE: &str = "none";

const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;
const MAX_PRACTICE_DURATION_MINUTES: i64 = 45;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeEligibilityV1 {
    pub schema: String,
    pub authority_kind: String,
    pub activation_eligible: bool,
    pub qualification_eligible: bool,
    pub scientific_finality_eligible: bool,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub score_eligible: bool,
    pub economic_eligible: bool,
    pub completion_portable: bool,
}

impl PracticeEligibilityV1 {
    pub fn locked() -> Self {
        Self {
            schema: PRACTICE_ELIGIBILITY_V1.to_owned(),
            authority_kind: PRACTICE_AUTHORITY_NONE.to_owned(),
            activation_eligible: false,
            qualification_eligible: false,
            scientific_finality_eligible: false,
            ranking_eligible: false,
            reward_eligible: false,
            score_eligible: false,
            economic_eligible: false,
            completion_portable: false,
        }
    }

    pub fn validate(&self) -> Result<(), PracticeError> {
        if self.schema != PRACTICE_ELIGIBILITY_V1 || self.authority_kind != PRACTICE_AUTHORITY_NONE
        {
            return Err(PracticeError::InvalidContract(
                "practice eligibility identity",
            ));
        }
        if self.activation_eligible
            || self.qualification_eligible
            || self.scientific_finality_eligible
            || self.ranking_eligible
            || self.reward_eligible
            || self.score_eligible
            || self.economic_eligible
            || self.completion_portable
        {
            return Err(PracticeError::AuthorityEscape);
        }
        Ok(())
    }
}

impl Default for PracticeEligibilityV1 {
    fn default() -> Self {
        Self::locked()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeStageV1 {
    CaptainPlan,
    EvidenceAssessment,
    ExperimentWaitingBridge,
    ExperimentInterpretation,
    CaptainAar,
    Completed,
    Abandoned,
    Expired,
}

impl PracticeStageV1 {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Abandoned | Self::Expired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeBridgeTaskStateV1 {
    Pending,
    Claimed,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptainPlanChoiceV1 {
    AuditHighestRiskClaim,
    AuditEvidenceChainFirst,
}

impl CaptainPlanChoiceV1 {
    fn code(self) -> &'static str {
        match self {
            Self::AuditHighestRiskClaim => "audit_highest_risk_claim",
            Self::AuditEvidenceChainFirst => "audit_evidence_chain_first",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAssessmentChoiceV1 {
    UnsupportedClaim,
    CitationMismatch,
    EvidenceSufficient,
}

impl EvidenceAssessmentChoiceV1 {
    fn code(self) -> &'static str {
        match self {
            Self::UnsupportedClaim => "unsupported_claim",
            Self::CitationMismatch => "citation_mismatch",
            Self::EvidenceSufficient => "evidence_sufficient",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeExperimentResultV1 {
    ConcernConfirmed,
    ConcernNotDetected,
    Inconclusive,
}

impl PracticeExperimentResultV1 {
    fn code(self) -> &'static str {
        match self {
            Self::ConcernConfirmed => "concern_confirmed",
            Self::ConcernNotDetected => "concern_not_detected",
            Self::Inconclusive => "inconclusive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentInterpretationChoiceV1 {
    ReviseClaim,
    RequestMoreEvidence,
    RetainClaimWithCaveat,
}

impl ExperimentInterpretationChoiceV1 {
    fn code(self) -> &'static str {
        match self {
            Self::ReviseClaim => "revise_claim",
            Self::RequestMoreEvidence => "request_more_evidence",
            Self::RetainClaimWithCaveat => "retain_claim_with_caveat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptainAarChoiceV1 {
    ImproveEvidenceTriage,
    ImproveExperimentDesign,
    ImproveTeamCoordination,
}

impl CaptainAarChoiceV1 {
    fn code(self) -> &'static str {
        match self {
            Self::ImproveEvidenceTriage => "improve_evidence_triage",
            Self::ImproveExperimentDesign => "improve_experiment_design",
            Self::ImproveTeamCoordination => "improve_team_coordination",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeTerminalReasonV1 {
    Completed,
    PlayerAbandoned,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeSessionV1 {
    pub schema: String,
    pub practice_session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub mode: String,
    pub scenario_id: String,
    pub eligibility: PracticeEligibilityV1,
    pub stage: PracticeStageV1,
    pub version: u64,
    pub captain_plan: Option<CaptainPlanChoiceV1>,
    pub evidence_assessment: Option<EvidenceAssessmentChoiceV1>,
    pub bridge_task_id: Uuid,
    pub bridge_task_state: PracticeBridgeTaskStateV1,
    pub bridge_result_code: Option<PracticeExperimentResultV1>,
    pub bridge_result_hash: Option<String>,
    pub experiment_interpretation: Option<ExperimentInterpretationChoiceV1>,
    pub aar_choice: Option<CaptainAarChoiceV1>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub terminal_reason: Option<PracticeTerminalReasonV1>,
}

impl PracticeSessionV1 {
    pub fn new(
        practice_session_id: Uuid,
        subject_id: String,
        player_id: Uuid,
        binding_id: Uuid,
        bridge_task_id: Uuid,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Result<Self, PracticeError> {
        let session = Self {
            schema: PRACTICE_SESSION_V1.to_owned(),
            practice_session_id,
            subject_id,
            player_id,
            binding_id,
            mode: PRACTICE_UNRANKED_MODE.to_owned(),
            scenario_id: PRACTICE_SCENARIO_EVIDENCE_AUDIT_INTRO_V1.to_owned(),
            eligibility: PracticeEligibilityV1::locked(),
            stage: PracticeStageV1::CaptainPlan,
            version: 1,
            captain_plan: None,
            evidence_assessment: None,
            bridge_task_id,
            bridge_task_state: PracticeBridgeTaskStateV1::Pending,
            bridge_result_code: None,
            bridge_result_hash: None,
            experiment_interpretation: None,
            aar_choice: None,
            created_at,
            expires_at,
            updated_at: created_at,
            terminal_at: None,
            terminal_reason: None,
        };
        session.validate()?;
        Ok(session)
    }

    pub fn validate(&self) -> Result<(), PracticeError> {
        if self.schema != PRACTICE_SESSION_V1
            || self.mode != PRACTICE_UNRANKED_MODE
            || self.scenario_id != PRACTICE_SCENARIO_EVIDENCE_AUDIT_INTRO_V1
        {
            return Err(PracticeError::InvalidContract("practice session identity"));
        }
        validate_non_nil(self.practice_session_id, "practice session id")?;
        validate_non_nil(self.player_id, "player id")?;
        validate_non_nil(self.binding_id, "binding id")?;
        validate_non_nil(self.bridge_task_id, "bridge task id")?;
        if self.subject_id.is_empty()
            || self.subject_id.len() > 256
            || self.subject_id.trim() != self.subject_id
        {
            return Err(PracticeError::InvalidContract("subject id"));
        }
        if self.version == 0 || self.version > JSON_SAFE_U64_MAX {
            return Err(PracticeError::InvalidContract("version"));
        }
        if self.expires_at <= self.created_at
            || self.expires_at > self.created_at + Duration::minutes(MAX_PRACTICE_DURATION_MINUTES)
            || self.updated_at < self.created_at
        {
            return Err(PracticeError::InvalidContract("practice timestamps"));
        }
        self.eligibility.validate()?;

        let bridge_result_pair = self.bridge_result_code.is_some()
            && self
                .bridge_result_hash
                .as_deref()
                .is_some_and(valid_digest_label);
        if self.bridge_result_code.is_some() != self.bridge_result_hash.is_some()
            || (self.bridge_result_hash.is_some() && !bridge_result_pair)
            || (self.bridge_task_state == PracticeBridgeTaskStateV1::Completed)
                != bridge_result_pair
        {
            return Err(PracticeError::InvalidContract("bridge result binding"));
        }
        if self.evidence_assessment.is_some() && self.captain_plan.is_none()
            || self.bridge_task_state != PracticeBridgeTaskStateV1::Pending
                && self.evidence_assessment.is_none()
            || self.experiment_interpretation.is_some()
                && (self.evidence_assessment.is_none() || !bridge_result_pair)
            || self.aar_choice.is_some() && self.experiment_interpretation.is_none()
        {
            return Err(PracticeError::InvalidContract("practice answer prefix"));
        }

        if self.stage.is_terminal() {
            if self.terminal_at != Some(self.updated_at)
                || self.terminal_reason
                    != Some(match self.stage {
                        PracticeStageV1::Completed => PracticeTerminalReasonV1::Completed,
                        PracticeStageV1::Abandoned => PracticeTerminalReasonV1::PlayerAbandoned,
                        PracticeStageV1::Expired => PracticeTerminalReasonV1::Expired,
                        _ => unreachable!("terminal stage already checked"),
                    })
            {
                return Err(PracticeError::InvalidContract("terminal binding"));
            }
        } else if self.terminal_at.is_some() || self.terminal_reason.is_some() {
            return Err(PracticeError::InvalidContract("active terminal binding"));
        }

        let active_shape = match self.stage {
            PracticeStageV1::CaptainPlan => {
                self.captain_plan.is_none()
                    && self.evidence_assessment.is_none()
                    && self.bridge_task_state == PracticeBridgeTaskStateV1::Pending
                    && self.experiment_interpretation.is_none()
                    && self.aar_choice.is_none()
            }
            PracticeStageV1::EvidenceAssessment => {
                self.captain_plan.is_some()
                    && self.evidence_assessment.is_none()
                    && self.bridge_task_state == PracticeBridgeTaskStateV1::Pending
                    && self.experiment_interpretation.is_none()
                    && self.aar_choice.is_none()
            }
            PracticeStageV1::ExperimentWaitingBridge => {
                self.captain_plan.is_some()
                    && self.evidence_assessment.is_some()
                    && matches!(
                        self.bridge_task_state,
                        PracticeBridgeTaskStateV1::Pending | PracticeBridgeTaskStateV1::Claimed
                    )
                    && self.experiment_interpretation.is_none()
                    && self.aar_choice.is_none()
            }
            PracticeStageV1::ExperimentInterpretation => {
                self.captain_plan.is_some()
                    && self.evidence_assessment.is_some()
                    && bridge_result_pair
                    && self.experiment_interpretation.is_none()
                    && self.aar_choice.is_none()
            }
            PracticeStageV1::CaptainAar => {
                self.captain_plan.is_some()
                    && self.evidence_assessment.is_some()
                    && bridge_result_pair
                    && self.experiment_interpretation.is_some()
                    && self.aar_choice.is_none()
            }
            PracticeStageV1::Completed => {
                self.captain_plan.is_some()
                    && self.evidence_assessment.is_some()
                    && bridge_result_pair
                    && self.experiment_interpretation.is_some()
                    && self.aar_choice.is_some()
            }
            PracticeStageV1::Abandoned | PracticeStageV1::Expired => true,
        };
        if !active_shape {
            return Err(PracticeError::InvalidContract("practice stage shape"));
        }
        Ok(())
    }

    pub fn apply_browser_action(
        &mut self,
        request: &PracticeBrowserActionRequestV1,
        now: DateTime<Utc>,
    ) -> Result<PracticeEventV1, PracticeError> {
        self.validate()?;
        request.validate()?;
        self.validate_request_owner(
            request.practice_session_id,
            &request.subject_id,
            request.player_id,
            request.binding_id,
            request.expected_version,
        )?;
        self.ensure_mutable(now)?;

        let from_version = self.version;
        let (event_kind, choice_code) = match request.action {
            PracticeBrowserActionV1::CaptainPlan { choice }
                if self.stage == PracticeStageV1::CaptainPlan =>
            {
                self.captain_plan = Some(choice);
                self.stage = PracticeStageV1::EvidenceAssessment;
                (PracticeEventKindV1::CaptainPlanned, Some(choice.code()))
            }
            PracticeBrowserActionV1::EvidenceAssessment { choice }
                if self.stage == PracticeStageV1::EvidenceAssessment =>
            {
                self.evidence_assessment = Some(choice);
                self.stage = PracticeStageV1::ExperimentWaitingBridge;
                (PracticeEventKindV1::EvidenceAssessed, Some(choice.code()))
            }
            PracticeBrowserActionV1::ExperimentInterpretation { choice }
                if self.stage == PracticeStageV1::ExperimentInterpretation =>
            {
                self.experiment_interpretation = Some(choice);
                self.stage = PracticeStageV1::CaptainAar;
                (
                    PracticeEventKindV1::ExperimentInterpreted,
                    Some(choice.code()),
                )
            }
            PracticeBrowserActionV1::CaptainAar { choice }
                if self.stage == PracticeStageV1::CaptainAar =>
            {
                self.aar_choice = Some(choice);
                self.stage = PracticeStageV1::Completed;
                self.terminal_at = Some(now);
                self.terminal_reason = Some(PracticeTerminalReasonV1::Completed);
                (PracticeEventKindV1::AarCompleted, Some(choice.code()))
            }
            PracticeBrowserActionV1::Abandon if !self.stage.is_terminal() => {
                self.stage = PracticeStageV1::Abandoned;
                self.terminal_at = Some(now);
                self.terminal_reason = Some(PracticeTerminalReasonV1::PlayerAbandoned);
                (PracticeEventKindV1::Abandoned, None)
            }
            _ => return Err(PracticeError::InvalidTransition),
        };
        self.advance(now)?;
        self.validate()?;
        PracticeEventV1::new(
            request.event_id,
            self.practice_session_id,
            PracticeActorKindV1::Browser,
            event_kind,
            from_version,
            self.version,
            request.request_hash.clone(),
            choice_code.map(str::to_owned),
            None,
            now,
        )
    }

    pub fn claim_bridge_task(
        &mut self,
        request: &PracticeBridgeClaimRequestV1,
        now: DateTime<Utc>,
    ) -> Result<PracticeEventV1, PracticeError> {
        self.validate()?;
        request.validate()?;
        self.validate_request_owner(
            request.practice_session_id,
            &request.subject_id,
            request.player_id,
            request.binding_id,
            request.expected_version,
        )?;
        if request.bridge_task_id != self.bridge_task_id {
            return Err(PracticeError::TaskMismatch);
        }
        self.ensure_mutable(now)?;
        if self.stage != PracticeStageV1::ExperimentWaitingBridge
            || self.bridge_task_state != PracticeBridgeTaskStateV1::Pending
        {
            return Err(PracticeError::InvalidTransition);
        }
        let from_version = self.version;
        self.bridge_task_state = PracticeBridgeTaskStateV1::Claimed;
        self.advance(now)?;
        self.validate()?;
        PracticeEventV1::new(
            request.event_id,
            self.practice_session_id,
            PracticeActorKindV1::Agent,
            PracticeEventKindV1::ExperimentClaimed,
            from_version,
            self.version,
            request.request_hash.clone(),
            None,
            None,
            now,
        )
    }

    pub fn apply_bridge_result(
        &mut self,
        request: &PracticeBridgeResultRequestV1,
        now: DateTime<Utc>,
    ) -> Result<PracticeEventV1, PracticeError> {
        self.validate()?;
        request.validate()?;
        self.validate_request_owner(
            request.practice_session_id,
            &request.subject_id,
            request.player_id,
            request.binding_id,
            request.expected_version,
        )?;
        if request.bridge_task_id != self.bridge_task_id {
            return Err(PracticeError::TaskMismatch);
        }
        self.ensure_mutable(now)?;
        if self.stage != PracticeStageV1::ExperimentWaitingBridge
            || self.bridge_task_state != PracticeBridgeTaskStateV1::Claimed
        {
            return Err(PracticeError::InvalidTransition);
        }
        let from_version = self.version;
        self.bridge_task_state = PracticeBridgeTaskStateV1::Completed;
        self.bridge_result_code = Some(request.result_code);
        self.bridge_result_hash = Some(request.result_hash.clone());
        self.stage = PracticeStageV1::ExperimentInterpretation;
        self.advance(now)?;
        self.validate()?;
        PracticeEventV1::new(
            request.event_id,
            self.practice_session_id,
            PracticeActorKindV1::Agent,
            PracticeEventKindV1::ExperimentCompleted,
            from_version,
            self.version,
            request.request_hash.clone(),
            Some(request.result_code.code().to_owned()),
            Some(request.result_hash.clone()),
            now,
        )
    }

    pub fn expire(
        &mut self,
        event_id: Uuid,
        request_hash: String,
        now: DateTime<Utc>,
    ) -> Result<PracticeEventV1, PracticeError> {
        self.validate()?;
        validate_non_nil(event_id, "event id")?;
        if !valid_digest_label(&request_hash) {
            return Err(PracticeError::InvalidContract("request hash"));
        }
        if self.stage.is_terminal() {
            return Err(PracticeError::Terminal);
        }
        if now < self.expires_at {
            return Err(PracticeError::NotExpired);
        }
        if now < self.updated_at {
            return Err(PracticeError::InvalidContract("non-monotonic time"));
        }
        if self.version >= JSON_SAFE_U64_MAX {
            return Err(PracticeError::VersionExhausted);
        }
        let from_version = self.version;
        self.stage = PracticeStageV1::Expired;
        self.terminal_at = Some(now);
        self.terminal_reason = Some(PracticeTerminalReasonV1::Expired);
        self.advance(now)?;
        self.validate()?;
        PracticeEventV1::new(
            event_id,
            self.practice_session_id,
            PracticeActorKindV1::Server,
            PracticeEventKindV1::Expired,
            from_version,
            self.version,
            request_hash,
            None,
            None,
            now,
        )
    }

    fn ensure_mutable(&self, now: DateTime<Utc>) -> Result<(), PracticeError> {
        if self.stage.is_terminal() {
            return Err(PracticeError::Terminal);
        }
        if now >= self.expires_at {
            return Err(PracticeError::Expired);
        }
        if now < self.updated_at {
            return Err(PracticeError::InvalidContract("non-monotonic time"));
        }
        if self.version >= JSON_SAFE_U64_MAX {
            return Err(PracticeError::VersionExhausted);
        }
        Ok(())
    }

    fn validate_request_owner(
        &self,
        practice_session_id: Uuid,
        subject_id: &str,
        player_id: Uuid,
        binding_id: Uuid,
        expected_version: u64,
    ) -> Result<(), PracticeError> {
        if practice_session_id != self.practice_session_id {
            return Err(PracticeError::SessionMismatch);
        }
        if subject_id != self.subject_id
            || player_id != self.player_id
            || binding_id != self.binding_id
        {
            return Err(PracticeError::OwnerMismatch);
        }
        if expected_version != self.version {
            return Err(PracticeError::StaleVersion);
        }
        Ok(())
    }

    fn advance(&mut self, now: DateTime<Utc>) -> Result<(), PracticeError> {
        self.version = self
            .version
            .checked_add(1)
            .filter(|version| *version <= JSON_SAFE_U64_MAX)
            .ok_or(PracticeError::VersionExhausted)?;
        self.updated_at = now;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PracticeBrowserActionV1 {
    CaptainPlan {
        choice: CaptainPlanChoiceV1,
    },
    EvidenceAssessment {
        choice: EvidenceAssessmentChoiceV1,
    },
    ExperimentInterpretation {
        choice: ExperimentInterpretationChoiceV1,
    },
    CaptainAar {
        choice: CaptainAarChoiceV1,
    },
    Abandon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeBrowserActionRequestV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub practice_session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub expected_version: u64,
    pub request_hash: String,
    pub action: PracticeBrowserActionV1,
}

impl PracticeBrowserActionRequestV1 {
    pub fn validate(&self) -> Result<(), PracticeError> {
        validate_request_envelope(PracticeRequestEnvelope {
            schema: &self.schema,
            expected_schema: PRACTICE_BROWSER_ACTION_V1,
            event_id: self.event_id,
            practice_session_id: self.practice_session_id,
            subject_id: &self.subject_id,
            player_id: self.player_id,
            binding_id: self.binding_id,
            expected_version: self.expected_version,
            request_hash: &self.request_hash,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeBridgeClaimRequestV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub practice_session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub bridge_task_id: Uuid,
    pub expected_version: u64,
    pub request_hash: String,
}

impl PracticeBridgeClaimRequestV1 {
    pub fn validate(&self) -> Result<(), PracticeError> {
        validate_request_envelope(PracticeRequestEnvelope {
            schema: &self.schema,
            expected_schema: PRACTICE_BRIDGE_CLAIM_V1,
            event_id: self.event_id,
            practice_session_id: self.practice_session_id,
            subject_id: &self.subject_id,
            player_id: self.player_id,
            binding_id: self.binding_id,
            expected_version: self.expected_version,
            request_hash: &self.request_hash,
        })?;
        validate_non_nil(self.bridge_task_id, "bridge task id")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeBridgeResultRequestV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub practice_session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub bridge_task_id: Uuid,
    pub expected_version: u64,
    pub request_hash: String,
    pub result_code: PracticeExperimentResultV1,
    pub result_hash: String,
}

impl PracticeBridgeResultRequestV1 {
    pub fn validate(&self) -> Result<(), PracticeError> {
        validate_request_envelope(PracticeRequestEnvelope {
            schema: &self.schema,
            expected_schema: PRACTICE_BRIDGE_RESULT_V1,
            event_id: self.event_id,
            practice_session_id: self.practice_session_id,
            subject_id: &self.subject_id,
            player_id: self.player_id,
            binding_id: self.binding_id,
            expected_version: self.expected_version,
            request_hash: &self.request_hash,
        })?;
        validate_non_nil(self.bridge_task_id, "bridge task id")?;
        if !valid_digest_label(&self.result_hash) {
            return Err(PracticeError::InvalidContract("result hash"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeActorKindV1 {
    Browser,
    Agent,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PracticeEventKindV1 {
    CaptainPlanned,
    EvidenceAssessed,
    ExperimentClaimed,
    ExperimentCompleted,
    ExperimentInterpreted,
    AarCompleted,
    Abandoned,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PracticeEventV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub practice_session_id: Uuid,
    pub actor_kind: PracticeActorKindV1,
    pub event_kind: PracticeEventKindV1,
    pub from_version: u64,
    pub to_version: u64,
    pub request_hash: String,
    pub choice_code: Option<String>,
    pub result_hash: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl PracticeEventV1 {
    #[allow(clippy::too_many_arguments)]
    fn new(
        event_id: Uuid,
        practice_session_id: Uuid,
        actor_kind: PracticeActorKindV1,
        event_kind: PracticeEventKindV1,
        from_version: u64,
        to_version: u64,
        request_hash: String,
        choice_code: Option<String>,
        result_hash: Option<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, PracticeError> {
        let event = Self {
            schema: PRACTICE_EVENT_V1.to_owned(),
            event_id,
            practice_session_id,
            actor_kind,
            event_kind,
            from_version,
            to_version,
            request_hash,
            choice_code,
            result_hash,
            occurred_at,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), PracticeError> {
        if self.schema != PRACTICE_EVENT_V1 {
            return Err(PracticeError::InvalidContract("event schema"));
        }
        validate_non_nil(self.event_id, "event id")?;
        validate_non_nil(self.practice_session_id, "practice session id")?;
        if self.from_version == 0
            || self.from_version >= JSON_SAFE_U64_MAX
            || self.to_version != self.from_version + 1
            || self.to_version > JSON_SAFE_U64_MAX
            || !valid_digest_label(&self.request_hash)
            || self
                .result_hash
                .as_deref()
                .is_some_and(|value| !valid_digest_label(value))
        {
            return Err(PracticeError::InvalidContract("practice event"));
        }
        let shape_valid = match self.event_kind {
            PracticeEventKindV1::CaptainPlanned => {
                self.actor_kind == PracticeActorKindV1::Browser
                    && matches!(
                        self.choice_code.as_deref(),
                        Some("audit_highest_risk_claim" | "audit_evidence_chain_first")
                    )
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::EvidenceAssessed => {
                self.actor_kind == PracticeActorKindV1::Browser
                    && matches!(
                        self.choice_code.as_deref(),
                        Some("unsupported_claim" | "citation_mismatch" | "evidence_sufficient")
                    )
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::ExperimentClaimed => {
                self.actor_kind == PracticeActorKindV1::Agent
                    && self.choice_code.is_none()
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::ExperimentCompleted => {
                self.actor_kind == PracticeActorKindV1::Agent
                    && matches!(
                        self.choice_code.as_deref(),
                        Some("concern_confirmed" | "concern_not_detected" | "inconclusive")
                    )
                    && self.result_hash.is_some()
            }
            PracticeEventKindV1::ExperimentInterpreted => {
                self.actor_kind == PracticeActorKindV1::Browser
                    && matches!(
                        self.choice_code.as_deref(),
                        Some("revise_claim" | "request_more_evidence" | "retain_claim_with_caveat")
                    )
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::AarCompleted => {
                self.actor_kind == PracticeActorKindV1::Browser
                    && matches!(
                        self.choice_code.as_deref(),
                        Some(
                            "improve_evidence_triage"
                                | "improve_experiment_design"
                                | "improve_team_coordination"
                        )
                    )
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::Abandoned => {
                self.actor_kind == PracticeActorKindV1::Browser
                    && self.choice_code.is_none()
                    && self.result_hash.is_none()
            }
            PracticeEventKindV1::Expired => {
                self.actor_kind == PracticeActorKindV1::Server
                    && self.choice_code.is_none()
                    && self.result_hash.is_none()
            }
        };
        if !shape_valid {
            return Err(PracticeError::InvalidContract("practice event shape"));
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PracticeError {
    #[error("invalid practice contract: {0}")]
    InvalidContract(&'static str),
    #[error("practice authority escape rejected")]
    AuthorityEscape,
    #[error("practice session mismatch")]
    SessionMismatch,
    #[error("practice owner mismatch")]
    OwnerMismatch,
    #[error("practice bridge task mismatch")]
    TaskMismatch,
    #[error("practice version is stale")]
    StaleVersion,
    #[error("practice session has expired")]
    Expired,
    #[error("practice session is not yet expired")]
    NotExpired,
    #[error("practice session is terminal")]
    Terminal,
    #[error("invalid practice transition")]
    InvalidTransition,
    #[error("practice version exhausted")]
    VersionExhausted,
}

struct PracticeRequestEnvelope<'a> {
    schema: &'a str,
    expected_schema: &'a str,
    event_id: Uuid,
    practice_session_id: Uuid,
    subject_id: &'a str,
    player_id: Uuid,
    binding_id: Uuid,
    expected_version: u64,
    request_hash: &'a str,
}

fn validate_request_envelope(request: PracticeRequestEnvelope<'_>) -> Result<(), PracticeError> {
    if request.schema != request.expected_schema {
        return Err(PracticeError::InvalidContract("request schema"));
    }
    validate_non_nil(request.event_id, "event id")?;
    validate_non_nil(request.practice_session_id, "practice session id")?;
    validate_non_nil(request.player_id, "player id")?;
    validate_non_nil(request.binding_id, "binding id")?;
    if request.subject_id.is_empty()
        || request.subject_id.len() > 256
        || request.subject_id.trim() != request.subject_id
    {
        return Err(PracticeError::InvalidContract("subject id"));
    }
    if request.expected_version == 0
        || request.expected_version > JSON_SAFE_U64_MAX
        || !valid_digest_label(request.request_hash)
    {
        return Err(PracticeError::InvalidContract("request version or hash"));
    }
    Ok(())
}

fn validate_non_nil(value: Uuid, field: &'static str) -> Result<(), PracticeError> {
    if value.is_nil() {
        return Err(PracticeError::InvalidContract(field));
    }
    Ok(())
}

fn valid_digest_label(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn digest(value: &str) -> String {
        format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
    }

    fn fixture() -> PracticeSessionV1 {
        let created_at = Utc::now();
        PracticeSessionV1::new(
            Uuid::new_v4(),
            "practice-player".to_owned(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            created_at,
            created_at + Duration::minutes(20),
        )
        .expect("valid fixture")
    }

    fn browser_request(
        session: &PracticeSessionV1,
        action: PracticeBrowserActionV1,
    ) -> PracticeBrowserActionRequestV1 {
        PracticeBrowserActionRequestV1 {
            schema: PRACTICE_BROWSER_ACTION_V1.to_owned(),
            event_id: Uuid::new_v4(),
            practice_session_id: session.practice_session_id,
            subject_id: session.subject_id.clone(),
            player_id: session.player_id,
            binding_id: session.binding_id,
            expected_version: session.version,
            request_hash: digest(&Uuid::new_v4().to_string()),
            action,
        }
    }

    #[test]
    fn exact_practice_sequence_completes_without_authority() {
        let mut session = fixture();
        let mut now = session.created_at + Duration::seconds(1);
        let event = session
            .apply_browser_action(
                &browser_request(
                    &session,
                    PracticeBrowserActionV1::CaptainPlan {
                        choice: CaptainPlanChoiceV1::AuditHighestRiskClaim,
                    },
                ),
                now,
            )
            .expect("captain plan");
        assert_eq!(event.from_version, 1);
        assert_eq!(session.stage, PracticeStageV1::EvidenceAssessment);

        now += Duration::seconds(1);
        session
            .apply_browser_action(
                &browser_request(
                    &session,
                    PracticeBrowserActionV1::EvidenceAssessment {
                        choice: EvidenceAssessmentChoiceV1::UnsupportedClaim,
                    },
                ),
                now,
            )
            .expect("evidence assessment");

        now += Duration::seconds(1);
        let claim = PracticeBridgeClaimRequestV1 {
            schema: PRACTICE_BRIDGE_CLAIM_V1.to_owned(),
            event_id: Uuid::new_v4(),
            practice_session_id: session.practice_session_id,
            subject_id: session.subject_id.clone(),
            player_id: session.player_id,
            binding_id: session.binding_id,
            bridge_task_id: session.bridge_task_id,
            expected_version: session.version,
            request_hash: digest("claim"),
        };
        session
            .claim_bridge_task(&claim, now)
            .expect("claim exact practice task");

        now += Duration::seconds(1);
        let result = PracticeBridgeResultRequestV1 {
            schema: PRACTICE_BRIDGE_RESULT_V1.to_owned(),
            event_id: Uuid::new_v4(),
            practice_session_id: session.practice_session_id,
            subject_id: session.subject_id.clone(),
            player_id: session.player_id,
            binding_id: session.binding_id,
            bridge_task_id: session.bridge_task_id,
            expected_version: session.version,
            request_hash: digest("result-request"),
            result_code: PracticeExperimentResultV1::ConcernConfirmed,
            result_hash: digest("bounded-practice-result"),
        };
        session
            .apply_bridge_result(&result, now)
            .expect("accept exact practice result");

        now += Duration::seconds(1);
        session
            .apply_browser_action(
                &browser_request(
                    &session,
                    PracticeBrowserActionV1::ExperimentInterpretation {
                        choice: ExperimentInterpretationChoiceV1::ReviseClaim,
                    },
                ),
                now,
            )
            .expect("interpret result");

        now += Duration::seconds(1);
        session
            .apply_browser_action(
                &browser_request(
                    &session,
                    PracticeBrowserActionV1::CaptainAar {
                        choice: CaptainAarChoiceV1::ImproveEvidenceTriage,
                    },
                ),
                now,
            )
            .expect("complete AAR");
        assert_eq!(session.stage, PracticeStageV1::Completed);
        assert_eq!(session.terminal_at, Some(now));
        assert_eq!(session.eligibility, PracticeEligibilityV1::locked());
        session.validate().expect("completed contract");
    }

    #[test]
    fn stale_skip_cross_owner_and_cross_task_are_rejected_without_mutation() {
        let mut session = fixture();
        let initial = session.clone();
        let mut stale = browser_request(
            &session,
            PracticeBrowserActionV1::CaptainPlan {
                choice: CaptainPlanChoiceV1::AuditHighestRiskClaim,
            },
        );
        stale.expected_version += 1;
        assert_eq!(
            session.apply_browser_action(&stale, session.created_at + Duration::seconds(1)),
            Err(PracticeError::StaleVersion)
        );
        assert_eq!(session, initial);

        let skip = browser_request(
            &session,
            PracticeBrowserActionV1::CaptainAar {
                choice: CaptainAarChoiceV1::ImproveTeamCoordination,
            },
        );
        assert_eq!(
            session.apply_browser_action(&skip, session.created_at + Duration::seconds(1)),
            Err(PracticeError::InvalidTransition)
        );
        assert_eq!(session, initial);

        let mut wrong_owner = browser_request(
            &session,
            PracticeBrowserActionV1::CaptainPlan {
                choice: CaptainPlanChoiceV1::AuditEvidenceChainFirst,
            },
        );
        wrong_owner.player_id = Uuid::new_v4();
        assert_eq!(
            session.apply_browser_action(&wrong_owner, session.created_at + Duration::seconds(1)),
            Err(PracticeError::OwnerMismatch)
        );

        let mut claim_session = fixture();
        claim_session.stage = PracticeStageV1::ExperimentWaitingBridge;
        claim_session.captain_plan = Some(CaptainPlanChoiceV1::AuditHighestRiskClaim);
        claim_session.evidence_assessment = Some(EvidenceAssessmentChoiceV1::UnsupportedClaim);
        claim_session.validate().expect("waiting fixture");
        let claim = PracticeBridgeClaimRequestV1 {
            schema: PRACTICE_BRIDGE_CLAIM_V1.to_owned(),
            event_id: Uuid::new_v4(),
            practice_session_id: claim_session.practice_session_id,
            subject_id: claim_session.subject_id.clone(),
            player_id: claim_session.player_id,
            binding_id: claim_session.binding_id,
            bridge_task_id: Uuid::new_v4(),
            expected_version: claim_session.version,
            request_hash: digest("wrong-task"),
        };
        assert_eq!(
            claim_session
                .claim_bridge_task(&claim, claim_session.created_at + Duration::seconds(1)),
            Err(PracticeError::TaskMismatch)
        );
    }

    #[test]
    fn every_authority_and_portability_escape_is_rejected() {
        for field in [
            "activation_eligible",
            "qualification_eligible",
            "scientific_finality_eligible",
            "ranking_eligible",
            "reward_eligible",
            "score_eligible",
            "economic_eligible",
            "completion_portable",
        ] {
            let mut value = serde_json::to_value(PracticeEligibilityV1::locked())
                .expect("serialize eligibility");
            value[field] = json!(true);
            let escaped: PracticeEligibilityV1 =
                serde_json::from_value(value).expect("known boolean field");
            assert_eq!(escaped.validate(), Err(PracticeError::AuthorityEscape));
        }
    }

    #[test]
    fn cross_domain_identifiers_are_unknown_fields() {
        let session = fixture();
        let base = serde_json::to_value(browser_request(
            &session,
            PracticeBrowserActionV1::CaptainPlan {
                choice: CaptainPlanChoiceV1::AuditHighestRiskClaim,
            },
        ))
        .expect("serialize request");
        for forbidden in [
            "paper_project_id",
            "challenge_id",
            "activation_id",
            "qualification_id",
            "submission_id",
            "release_candidate_hash",
            "paper_bundle_hash",
            "finality_receipt_hash",
            "rank",
            "reward",
            "economy",
        ] {
            let mut hostile = base.clone();
            hostile[forbidden] = json!(Uuid::new_v4());
            assert!(
                serde_json::from_value::<PracticeBrowserActionRequestV1>(hostile).is_err(),
                "accepted cross-domain field {forbidden}"
            );
        }
    }

    #[test]
    fn terminal_expiry_and_abandonment_are_fail_closed() {
        let mut abandoned = fixture();
        let abandon = browser_request(&abandoned, PracticeBrowserActionV1::Abandon);
        let now = abandoned.created_at + Duration::seconds(1);
        abandoned
            .apply_browser_action(&abandon, now)
            .expect("abandon practice");
        let replay = browser_request(&abandoned, PracticeBrowserActionV1::Abandon);
        assert_eq!(
            abandoned.apply_browser_action(&replay, now + Duration::seconds(1)),
            Err(PracticeError::Terminal)
        );

        let mut expired = fixture();
        let before_expiry = expired.expires_at - Duration::seconds(1);
        assert_eq!(
            expired.expire(Uuid::new_v4(), digest("early"), before_expiry),
            Err(PracticeError::NotExpired)
        );
        let expiry = expired.expires_at;
        expired
            .expire(Uuid::new_v4(), digest("expiry"), expiry)
            .expect("expire at boundary");
        assert_eq!(expired.stage, PracticeStageV1::Expired);
        assert_eq!(expired.terminal_at, Some(expiry));

        let mut forged_terminal = fixture();
        forged_terminal.stage = PracticeStageV1::Abandoned;
        forged_terminal.bridge_task_state = PracticeBridgeTaskStateV1::Completed;
        forged_terminal.bridge_result_code = Some(PracticeExperimentResultV1::ConcernConfirmed);
        forged_terminal.bridge_result_hash = Some(digest("forged terminal result"));
        forged_terminal.updated_at += Duration::seconds(1);
        forged_terminal.terminal_at = Some(forged_terminal.updated_at);
        forged_terminal.terminal_reason = Some(PracticeTerminalReasonV1::PlayerAbandoned);
        assert!(forged_terminal.validate().is_err());

        let mut exhausted = fixture();
        exhausted.version = JSON_SAFE_U64_MAX;
        let before = exhausted.clone();
        let request = browser_request(
            &exhausted,
            PracticeBrowserActionV1::CaptainPlan {
                choice: CaptainPlanChoiceV1::AuditHighestRiskClaim,
            },
        );
        assert_eq!(
            exhausted.apply_browser_action(&request, exhausted.created_at + Duration::seconds(1)),
            Err(PracticeError::VersionExhausted)
        );
        assert_eq!(exhausted, before);
        assert!(PracticeEventV1::new(
            Uuid::new_v4(),
            exhausted.practice_session_id,
            PracticeActorKindV1::Browser,
            PracticeEventKindV1::Abandoned,
            JSON_SAFE_U64_MAX,
            JSON_SAFE_U64_MAX,
            digest("overflow"),
            None,
            None,
            exhausted.created_at,
        )
        .is_err());
    }

    #[tokio::test]
    async fn real_postgres_practice_catalog_lifecycle_and_append_only_gate() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!(
                "PAPER_RAID_BFF_TEST_DATABASE_URL is unset; practice PostgreSQL gate skipped"
            );
            return;
        };
        let pool = crate::db::connect(&database_url)
            .await
            .expect("connect practice test PostgreSQL");
        crate::db::migrate(&pool)
            .await
            .expect("migrate practice test PostgreSQL");
        assert!(
            crate::db::practice_unranked_schema_ready(&pool).await,
            "practice catalog must be exact after migration"
        );

        let now = Utc::now();
        let subject_id = format!("practice-subject-{}", Uuid::new_v4());
        let player_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let grant_id = Uuid::new_v4();
        let agent_id = format!("practice-agent-{}", Uuid::new_v4());
        let agent_key_id = digest("practice-agent-key");
        let capability_disclosure_hash = digest("practice-capability-disclosure");
        let capability_disclosure = json!({
            "schema": "hepta.paper_raid.agent_capability_disclosure.v1",
            "assurance": "self_declared_unverified",
            "capabilities": ["experiment_execution"],
            "resource_classes": ["cpu", "sandbox"],
            "max_parallel_tasks": 1,
        });
        let binding_record = json!({
            "binding_id": binding_id,
            "player_id": player_id,
            "agent_id": agent_id,
            "agent_key_id": agent_key_id,
            "capability_disclosure_hash": capability_disclosure_hash,
            "capability_disclosure": capability_disclosure,
            "status": "active",
        });
        let code_hash = Sha256::digest(Uuid::new_v4().as_bytes()).to_vec();
        let pinned_request_hash = Sha256::digest(Uuid::new_v4().as_bytes()).to_vec();
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_pairing_grants ( \
                grant_id, subject_id, player_id, code_hash, state, \
                pinned_request_hash, pinned_binding_id, created_at, expires_at, \
                pinned_at, consumed_at, pair_response_status, pair_response_body, updated_at \
             ) VALUES ($1,$2,$3,$4,'consumed',$5,$6,$7,$8,$7,$7,200,$9,$7)",
        )
        .bind(grant_id)
        .bind(&subject_id)
        .bind(player_id)
        .bind(code_hash)
        .bind(pinned_request_hash)
        .bind(binding_id)
        .bind(now)
        .bind(now + Duration::minutes(4))
        .bind(b"{}".as_slice())
        .execute(&pool)
        .await
        .expect("insert consumed pairing grant fixture");
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_bridge_bindings ( \
                binding_id, grant_id, last_pairing_grant_id, subject_id, player_id, \
                agent_id, agent_key_id, capability_disclosure_hash, \
                capability_disclosure, binding_record, paired_at, last_verified_at \
             ) VALUES ($1,$2,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
        )
        .bind(binding_id)
        .bind(grant_id)
        .bind(&subject_id)
        .bind(player_id)
        .bind(&agent_id)
        .bind(&agent_key_id)
        .bind(&capability_disclosure_hash)
        .bind(&capability_disclosure)
        .bind(&binding_record)
        .bind(now)
        .execute(&pool)
        .await
        .expect("insert exact practice binding owner fixture");

        let practice_session_id = Uuid::new_v4();
        let bridge_task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO paper_raid_bff_practice_sessions ( \
                practice_session_id, subject_id, player_id, binding_id, scenario_id, \
                stage, version, bridge_task_id, bridge_task_state, \
                created_at, expires_at, updated_at \
             ) VALUES ($1,$2,$3,$4,'evidence-audit-intro-v1', \
                'captain_plan',1,$5,'pending',$6,$7,$6)",
        )
        .bind(practice_session_id)
        .bind(&subject_id)
        .bind(player_id)
        .bind(binding_id)
        .bind(bridge_task_id)
        .bind(now)
        .bind(now + Duration::minutes(20))
        .execute(&pool)
        .await
        .expect("insert valid practice session");

        let authority_escape = sqlx::query(
            "INSERT INTO paper_raid_bff_practice_sessions ( \
                practice_session_id, subject_id, player_id, binding_id, scenario_id, \
                stage, version, bridge_task_id, bridge_task_state, ranking_eligible, \
                created_at, expires_at, updated_at, terminal_at, terminal_reason \
             ) VALUES ($1,$2,$3,$4,'evidence-audit-intro-v1', \
                'expired',1,$5,'pending',TRUE,$6,$7,$6,$6,'expired')",
        )
        .bind(Uuid::new_v4())
        .bind(&subject_id)
        .bind(player_id)
        .bind(binding_id)
        .bind(Uuid::new_v4())
        .bind(now)
        .bind(now + Duration::minutes(20))
        .execute(&pool)
        .await;
        assert!(
            authority_escape.is_err(),
            "database accepted ranking authority"
        );

        let cross_owner = sqlx::query(
            "INSERT INTO paper_raid_bff_practice_sessions ( \
                practice_session_id, subject_id, player_id, binding_id, scenario_id, \
                stage, version, bridge_task_id, bridge_task_state, \
                created_at, expires_at, updated_at, terminal_at, terminal_reason \
             ) VALUES ($1,$2,$3,$4,'evidence-audit-intro-v1', \
                'expired',1,$5,'pending',$6,$7,$6,$6,'expired')",
        )
        .bind(Uuid::new_v4())
        .bind(&subject_id)
        .bind(Uuid::new_v4())
        .bind(binding_id)
        .bind(Uuid::new_v4())
        .bind(now)
        .bind(now + Duration::minutes(20))
        .execute(&pool)
        .await;
        assert!(
            cross_owner.is_err(),
            "database accepted a cross-owner binding"
        );

        let transition_at = now + Duration::seconds(1);
        sqlx::query(
            "UPDATE paper_raid_bff_practice_sessions \
                SET stage='evidence_assessment', \
                    captain_plan='audit_highest_risk_claim', \
                    version=2, updated_at=$2 \
              WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .bind(transition_at)
        .execute(&pool)
        .await
        .expect("accept legal captain transition");
        let event_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO paper_raid_bff_practice_events ( \
                event_id, practice_session_id, actor_kind, event_kind, \
                from_version, to_version, request_hash, choice_code, occurred_at \
             ) VALUES ($1,$2,'browser','captain_planned',1,2,$3, \
                'audit_highest_risk_claim',$4)",
        )
        .bind(event_id)
        .bind(practice_session_id)
        .bind(digest("captain transition"))
        .bind(transition_at)
        .execute(&pool)
        .await
        .expect("append exact practice event");

        let stale = sqlx::query(
            "UPDATE paper_raid_bff_practice_sessions \
                SET stage='experiment_waiting_bridge', \
                    evidence_assessment='unsupported_claim', \
                    version=4, updated_at=$2 \
              WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .bind(now + Duration::seconds(2))
        .execute(&pool)
        .await;
        assert!(stale.is_err(), "database accepted a stale version jump");
        let skip = sqlx::query(
            "UPDATE paper_raid_bff_practice_sessions \
                SET stage='experiment_interpretation', \
                    evidence_assessment='unsupported_claim', \
                    bridge_task_state='completed', \
                    bridge_result_code='concern_confirmed', \
                    bridge_result_hash=$2, version=3, updated_at=$3 \
              WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .bind(digest("skipped result"))
        .bind(now + Duration::seconds(2))
        .execute(&pool)
        .await;
        assert!(skip.is_err(), "database accepted a skipped stage");
        let terminal_mutation = sqlx::query(
            "UPDATE paper_raid_bff_practice_sessions \
                SET stage='abandoned', evidence_assessment='unsupported_claim', \
                    version=3, updated_at=$2, terminal_at=$2, \
                    terminal_reason='player_abandoned' \
              WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .bind(now + Duration::seconds(2))
        .execute(&pool)
        .await;
        assert!(
            terminal_mutation.is_err(),
            "database accepted answer mutation during abandonment"
        );
        let early_expiry = sqlx::query(
            "UPDATE paper_raid_bff_practice_sessions \
                SET stage='expired', version=3, updated_at=$2, terminal_at=$2, \
                    terminal_reason='expired' \
              WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .bind(now + Duration::seconds(2))
        .execute(&pool)
        .await;
        assert!(early_expiry.is_err(), "database accepted early expiry");
        let stored_version = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM paper_raid_bff_practice_sessions \
             WHERE practice_session_id=$1",
        )
        .bind(practice_session_id)
        .fetch_one(&pool)
        .await
        .expect("read practice version after hostile updates");
        assert_eq!(stored_version, 2);

        assert!(
            sqlx::query(
                "UPDATE paper_raid_bff_practice_events SET actor_kind='server' WHERE event_id=$1"
            )
            .bind(event_id)
            .execute(&pool)
            .await
            .is_err(),
            "practice event update was not rejected"
        );
        assert!(
            sqlx::query("DELETE FROM paper_raid_bff_practice_events WHERE event_id=$1")
                .bind(event_id)
                .execute(&pool)
                .await
                .is_err(),
            "practice event delete was not rejected"
        );
        assert!(
            sqlx::query("TRUNCATE paper_raid_bff_practice_events")
                .execute(&pool)
                .await
                .is_err(),
            "practice event truncate was not rejected"
        );

        crate::db::migrate(&pool)
            .await
            .expect("practice migration remains idempotent");
        assert!(crate::db::practice_unranked_schema_ready(&pool).await);
    }
}
