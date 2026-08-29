//! The bounded Quick Raid contract.
//!
//! Quick Raid is intentionally a small, fixed-seed slice of Paper Raid.  It is
//! not a second scoring system and it never mints a portable PaperBundle.  The
//! only durable authority it creates is an append-only, paper-scoped
//! projection containing one EvidenceCard, one deterministic Experiment Run,
//! and a player-visible bundle preview.  Ranking, reward, score, economic, and
//! scientific-finality flags are hard locked to false in both Rust and the
//! BFF migration.

use chrono::{DateTime, Duration, Utc};
use hepta_paper_raid_contracts::{canonical_json_sha256, sha256_digest};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const QUICK_RAID_MODE: &str = "quick_raid";
pub const QUICK_RAID_SCENARIO_V1: &str = "evidence-audit-quick-v1";
pub const QUICK_RAID_SESSION_V1: &str = "hepta.paper_raid.quick_raid_session.v1";
pub const QUICK_RAID_EVENT_V1: &str = "hepta.paper_raid.quick_raid_event.v1";
pub const QUICK_RAID_ACTION_V1: &str = "hepta.paper_raid.quick_raid_action.v1";
pub const QUICK_RAID_VIEW_V1: &str = "hepta.paper_raid.quick_raid_player_view.v1";
pub const QUICK_RAID_BRIEF_V1: &str = "hepta.paper_raid.quick_raid_brief.v1";
pub const QUICK_RAID_EVIDENCE_CARD_V1: &str = "hepta.paper_raid.quick_raid_evidence_card.v1";
pub const QUICK_RAID_RUN_V1: &str = "hepta.paper_raid.quick_raid_run.v1";
pub const QUICK_RAID_PAPER_BUNDLE_V1: &str = "hepta.paper_raid.quick_raid_paper_bundle.v1";
pub const QUICK_RAID_AUTHORITY_KIND: &str = "hepta_challenge_pack_projection_v1";
pub const QUICK_RAID_PACK_ID: &str = "paper-raid-evidence-audit-quick-seeded-v1";
pub const QUICK_RAID_RULESET_VERSION: &str = "paper-raid-evidence-audit-quick-v1";
pub const QUICK_RAID_FIXED_SEED: u64 = 17;
pub const QUICK_RAID_DURATION_MINUTES: i64 = 15;
pub const QUICK_RAID_DURATION_SECONDS: u32 = 15 * 60;
pub const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;

/// This is a stable logical id, not a player-controlled challenge id.  The
/// live Hepta challenge id and activation hash are frozen into the authority
/// returned by the challenge catalog before a session is created.
pub const QUICK_RAID_CHALLENGE_KEY: &str = "evidence-audit-quick";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidEligibilityV1 {
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

impl QuickRaidEligibilityV1 {
    pub fn locked() -> Self {
        Self {
            schema: "hepta.paper_raid.quick_raid_eligibility.v1".to_owned(),
            authority_kind: QUICK_RAID_AUTHORITY_KIND.to_owned(),
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

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self.schema != "hepta.paper_raid.quick_raid_eligibility.v1"
            || self.authority_kind != QUICK_RAID_AUTHORITY_KIND
        {
            return Err(QuickRaidError::InvalidContract("eligibility identity"));
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
            return Err(QuickRaidError::AuthorityEscape);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidAuthorityV1 {
    pub schema: String,
    pub authority_kind: String,
    pub challenge_key: String,
    pub challenge_id: Uuid,
    pub challenge_snapshot_hash: String,
    pub pack_id: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub seed: u64,
    pub duration_seconds: u32,
    pub authority_hash: String,
}

impl QuickRaidAuthorityV1 {
    pub fn from_catalog(challenge: &serde_json::Value) -> Result<Self, QuickRaidError> {
        let challenge_id = canonical_uuid(challenge.get("challenge_id"))?;
        if challenge.get("status").and_then(serde_json::Value::as_str) != Some("open") {
            return Err(QuickRaidError::ChallengeUnavailable);
        }
        if challenge
            .get("ruleset_enforcement")
            .and_then(serde_json::Value::as_str)
            != Some("authoritative_v1")
        {
            return Err(QuickRaidError::ChallengeUnavailable);
        }
        let ruleset = challenge
            .get("ruleset")
            .filter(|value| value.is_object())
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        let template = ruleset
            .get("template")
            .and_then(serde_json::Value::as_str)
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        if template != "evidence-audit" {
            return Err(QuickRaidError::ChallengeUnavailable);
        }
        let duration_seconds = ruleset
            .get("duration_seconds")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        if duration_seconds != QUICK_RAID_DURATION_SECONDS {
            return Err(QuickRaidError::ChallengeUnavailable);
        }
        let gameplay = ruleset
            .get("gameplay")
            .filter(|value| value.is_object())
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        let modifiers = gameplay
            .get("modifiers")
            .and_then(serde_json::Value::as_array)
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        if !modifiers
            .iter()
            .any(|modifier| modifier.as_str() == Some("quick-raid-fixed-seed"))
        {
            return Err(QuickRaidError::ChallengeUnavailable);
        }
        let pack_id = challenge
            .get("description")
            .and_then(serde_json::Value::as_str)
            .and_then(|description| {
                description
                    .split(';')
                    .find_map(|part| part.trim().strip_prefix("pack_id="))
            })
            .filter(|value| *value == QUICK_RAID_PACK_ID)
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        let ruleset_version = challenge
            .get("ruleset_version")
            .and_then(serde_json::Value::as_str)
            .filter(|value| *value == QUICK_RAID_RULESET_VERSION)
            .ok_or(QuickRaidError::ChallengeUnavailable)?;
        let ruleset_hash = digest(challenge.get("ruleset_hash"))?;
        let challenge_snapshot_hash =
            canonical_json_sha256(challenge).map_err(|_| QuickRaidError::ChallengeUnavailable)?;
        let mut authority = Self {
            schema: "hepta.paper_raid.quick_raid_authority.v1".to_owned(),
            authority_kind: QUICK_RAID_AUTHORITY_KIND.to_owned(),
            challenge_key: QUICK_RAID_CHALLENGE_KEY.to_owned(),
            challenge_id,
            challenge_snapshot_hash,
            pack_id: pack_id.to_owned(),
            ruleset_version: ruleset_version.to_owned(),
            ruleset_hash,
            seed: QUICK_RAID_FIXED_SEED,
            duration_seconds,
            authority_hash: String::new(),
        };
        authority.authority_hash = canonical_json_sha256(&authority_without_hash(&authority))
            .map_err(|_| QuickRaidError::InvalidContract("authority hash"))?;
        authority.validate()?;
        Ok(authority)
    }

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self.schema != "hepta.paper_raid.quick_raid_authority.v1"
            || self.authority_kind != QUICK_RAID_AUTHORITY_KIND
            || self.challenge_key != QUICK_RAID_CHALLENGE_KEY
            || self.pack_id != QUICK_RAID_PACK_ID
            || self.ruleset_version != QUICK_RAID_RULESET_VERSION
            || self.seed != QUICK_RAID_FIXED_SEED
            || self.duration_seconds != QUICK_RAID_DURATION_SECONDS
        {
            return Err(QuickRaidError::InvalidContract("authority identity"));
        }
        canonical_uuid_text(self.challenge_id)?;
        valid_digest(&self.challenge_snapshot_hash)?;
        valid_digest(&self.ruleset_hash)?;
        valid_digest(&self.authority_hash)?;
        let expected = canonical_json_sha256(&authority_without_hash(self))
            .map_err(|_| QuickRaidError::InvalidContract("authority hash"))?;
        if expected != self.authority_hash {
            return Err(QuickRaidError::InvalidContract("authority hash"));
        }
        Ok(())
    }
}

fn authority_without_hash(authority: &QuickRaidAuthorityV1) -> QuickRaidAuthorityV1 {
    let mut copy = authority.clone();
    copy.authority_hash.clear();
    copy
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidEvidenceCardV1 {
    pub schema: String,
    pub evidence_card_id: String,
    pub seed: u64,
    pub claim: String,
    pub observation: String,
    pub citation: String,
    pub source_digest: String,
}

impl QuickRaidEvidenceCardV1 {
    pub fn fixed() -> Self {
        let source_digest =
            sha256_digest(b"quick-raid/evidence-audit/seed-17/source-observation-v1");
        Self {
            schema: QUICK_RAID_EVIDENCE_CARD_V1.to_owned(),
            evidence_card_id: "evidence-card-seed-17".to_owned(),
            seed: QUICK_RAID_FIXED_SEED,
            claim: "The candidate retrieval change improves evidence-supported precision."
                .to_owned(),
            observation:
                "The supplied citation covers the baseline but not the claimed improvement."
                    .to_owned(),
            citation: "quick://evidence-audit/seed-17/observation-01".to_owned(),
            source_digest,
        }
    }

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self != &Self::fixed() {
            return Err(QuickRaidError::InvalidContract("fixed EvidenceCard"));
        }
        valid_digest(&self.source_digest)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidEvidenceChoiceV1 {
    FlagCitationGap,
    AcceptAsSufficient,
}

impl QuickRaidEvidenceChoiceV1 {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::FlagCitationGap => "flag_citation_gap",
            Self::AcceptAsSufficient => "accept_as_sufficient",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidExperimentChoiceV1 {
    RecheckBaseline,
    RunCandidate,
}

impl QuickRaidExperimentChoiceV1 {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::RecheckBaseline => "recheck_baseline",
            Self::RunCandidate => "run_candidate",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidConclusionV1 {
    ReviseClaim,
    RetainWithCaveat,
}

impl QuickRaidConclusionV1 {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ReviseClaim => "revise_claim",
            Self::RetainWithCaveat => "retain_with_caveat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidExperimentRunV1 {
    pub schema: String,
    pub run_id: Uuid,
    pub seed: u64,
    pub choice: QuickRaidExperimentChoiceV1,
    pub result: String,
    pub metrics: Vec<QuickRaidMetricV1>,
    pub run_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidMetricV1 {
    pub name: String,
    pub baseline_bps: u32,
    pub candidate_bps: u32,
}

impl QuickRaidExperimentRunV1 {
    pub fn deterministic(run_id: Uuid, choice: QuickRaidExperimentChoiceV1) -> Self {
        let metrics = match choice {
            QuickRaidExperimentChoiceV1::RecheckBaseline => vec![QuickRaidMetricV1 {
                name: "evidence_supported_precision".to_owned(),
                baseline_bps: 7400,
                candidate_bps: 7400,
            }],
            QuickRaidExperimentChoiceV1::RunCandidate => vec![QuickRaidMetricV1 {
                name: "evidence_supported_precision".to_owned(),
                baseline_bps: 7400,
                candidate_bps: 7800,
            }],
        };
        let result = match choice {
            QuickRaidExperimentChoiceV1::RecheckBaseline => "baseline_reproduced",
            QuickRaidExperimentChoiceV1::RunCandidate => "candidate_improvement_observed",
        };
        let mut run = Self {
            schema: QUICK_RAID_RUN_V1.to_owned(),
            run_id,
            seed: QUICK_RAID_FIXED_SEED,
            choice,
            result: result.to_owned(),
            metrics,
            run_hash: String::new(),
        };
        run.run_hash = canonical_json_sha256(&run_without_hash(&run)).unwrap_or_default();
        run
    }

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self.schema != QUICK_RAID_RUN_V1
            || self.seed != QUICK_RAID_FIXED_SEED
            || self.metrics.len() != 1
            || self.metrics[0].name != "evidence_supported_precision"
        {
            return Err(QuickRaidError::InvalidContract("experiment run identity"));
        }
        canonical_uuid_text(self.run_id)?;
        valid_digest(&self.run_hash)?;
        let expected = canonical_json_sha256(&run_without_hash(self))
            .map_err(|_| QuickRaidError::InvalidContract("run hash"))?;
        if expected != self.run_hash {
            return Err(QuickRaidError::InvalidContract("run hash"));
        }
        let expected_run = Self::deterministic(self.run_id, self.choice);
        if self != &expected_run {
            return Err(QuickRaidError::InvalidContract(
                "deterministic experiment run",
            ));
        }
        Ok(())
    }
}

fn run_without_hash(run: &QuickRaidExperimentRunV1) -> QuickRaidExperimentRunV1 {
    let mut copy = run.clone();
    copy.run_hash.clear();
    copy
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidPaperBundleV1 {
    pub schema: String,
    pub authority: QuickRaidAuthorityV1,
    pub session_id: Uuid,
    pub seed: u64,
    pub evidence_card: QuickRaidEvidenceCardV1,
    pub experiment_run: QuickRaidExperimentRunV1,
    pub conclusion: QuickRaidConclusionV1,
    pub finality: String,
    pub portable: bool,
    pub paper_bundle_hash: String,
}

impl QuickRaidPaperBundleV1 {
    pub fn build(
        authority: QuickRaidAuthorityV1,
        session_id: Uuid,
        experiment_run: QuickRaidExperimentRunV1,
        conclusion: QuickRaidConclusionV1,
    ) -> Result<Self, QuickRaidError> {
        authority.validate()?;
        experiment_run.validate()?;
        let mut bundle = Self {
            schema: QUICK_RAID_PAPER_BUNDLE_V1.to_owned(),
            authority,
            session_id,
            seed: QUICK_RAID_FIXED_SEED,
            evidence_card: QuickRaidEvidenceCardV1::fixed(),
            experiment_run,
            conclusion,
            finality: "none".to_owned(),
            portable: false,
            paper_bundle_hash: String::new(),
        };
        bundle.paper_bundle_hash = canonical_json_sha256(&bundle_without_hash(&bundle))
            .map_err(|_| QuickRaidError::InvalidContract("paper bundle hash"))?;
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self.schema != QUICK_RAID_PAPER_BUNDLE_V1
            || self.seed != QUICK_RAID_FIXED_SEED
            || self.finality != "none"
            || self.portable
        {
            return Err(QuickRaidError::AuthorityEscape);
        }
        canonical_uuid_text(self.session_id)?;
        self.authority.validate()?;
        self.evidence_card.validate()?;
        self.experiment_run.validate()?;
        valid_digest(&self.paper_bundle_hash)?;
        let expected = canonical_json_sha256(&bundle_without_hash(self))
            .map_err(|_| QuickRaidError::InvalidContract("paper bundle hash"))?;
        if expected != self.paper_bundle_hash {
            return Err(QuickRaidError::InvalidContract("paper bundle hash"));
        }
        Ok(())
    }
}

fn bundle_without_hash(bundle: &QuickRaidPaperBundleV1) -> QuickRaidPaperBundleV1 {
    let mut copy = bundle.clone();
    copy.paper_bundle_hash.clear();
    copy
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidStageV1 {
    EvidenceReview,
    ExperimentRun,
    PaperBundleReady,
    Completed,
    Abandoned,
    Expired,
}

impl QuickRaidStageV1 {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Abandoned | Self::Expired)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidTerminalReasonV1 {
    Completed,
    PlayerAbandoned,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuickRaidBrowserActionV1 {
    ReviewEvidence { choice: QuickRaidEvidenceChoiceV1 },
    RunExperiment { choice: QuickRaidExperimentChoiceV1 },
    PublishPaper { conclusion: QuickRaidConclusionV1 },
    Abandon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidActionRequestV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub expected_version: u64,
    pub request_hash: String,
    pub action: QuickRaidBrowserActionV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidActorKindV1 {
    Browser,
    Server,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickRaidEventKindV1 {
    EvidenceReviewed,
    ExperimentRun,
    PaperPublished,
    Abandoned,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidEventV1 {
    pub schema: String,
    pub event_id: Uuid,
    pub session_id: Uuid,
    pub actor_kind: QuickRaidActorKindV1,
    pub event_kind: QuickRaidEventKindV1,
    pub from_version: u64,
    pub to_version: u64,
    pub request_hash: String,
    pub choice_code: Option<String>,
    pub result_hash: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuickRaidSessionV1 {
    pub schema: String,
    pub session_id: Uuid,
    pub subject_id: String,
    pub player_id: Uuid,
    pub binding_id: Uuid,
    pub mode: String,
    pub scenario_id: String,
    pub authority: QuickRaidAuthorityV1,
    pub eligibility: QuickRaidEligibilityV1,
    pub stage: QuickRaidStageV1,
    pub version: u64,
    pub evidence_card: QuickRaidEvidenceCardV1,
    pub evidence_choice: Option<QuickRaidEvidenceChoiceV1>,
    pub experiment_choice: Option<QuickRaidExperimentChoiceV1>,
    pub experiment_run: Option<QuickRaidExperimentRunV1>,
    pub conclusion: Option<QuickRaidConclusionV1>,
    pub paper_bundle: Option<QuickRaidPaperBundleV1>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminal_at: Option<DateTime<Utc>>,
    pub terminal_reason: Option<QuickRaidTerminalReasonV1>,
}

impl QuickRaidSessionV1 {
    pub fn new(
        session_id: Uuid,
        subject_id: String,
        player_id: Uuid,
        binding_id: Uuid,
        authority: QuickRaidAuthorityV1,
        now: DateTime<Utc>,
    ) -> Result<Self, QuickRaidError> {
        authority.validate()?;
        let session = Self {
            schema: QUICK_RAID_SESSION_V1.to_owned(),
            session_id,
            subject_id,
            player_id,
            binding_id,
            mode: QUICK_RAID_MODE.to_owned(),
            scenario_id: QUICK_RAID_SCENARIO_V1.to_owned(),
            authority,
            eligibility: QuickRaidEligibilityV1::locked(),
            stage: QuickRaidStageV1::EvidenceReview,
            version: 1,
            evidence_card: QuickRaidEvidenceCardV1::fixed(),
            evidence_choice: None,
            experiment_choice: None,
            experiment_run: None,
            conclusion: None,
            paper_bundle: None,
            created_at: now,
            expires_at: now + Duration::minutes(QUICK_RAID_DURATION_MINUTES),
            updated_at: now,
            terminal_at: None,
            terminal_reason: None,
        };
        session.validate()?;
        Ok(session)
    }

    pub fn validate(&self) -> Result<(), QuickRaidError> {
        if self.schema != QUICK_RAID_SESSION_V1
            || self.mode != QUICK_RAID_MODE
            || self.scenario_id != QUICK_RAID_SCENARIO_V1
        {
            return Err(QuickRaidError::InvalidContract("session identity"));
        }
        canonical_uuid_text(self.session_id)?;
        canonical_uuid_text(self.player_id)?;
        canonical_uuid_text(self.binding_id)?;
        if self.subject_id.is_empty()
            || self.subject_id.len() > 256
            || self.subject_id.trim() != self.subject_id
        {
            return Err(QuickRaidError::InvalidContract("subject id"));
        }
        if self.version == 0 || self.version > JSON_SAFE_U64_MAX {
            return Err(QuickRaidError::InvalidContract("version"));
        }
        if self.expires_at != self.created_at + Duration::minutes(QUICK_RAID_DURATION_MINUTES)
            || self.updated_at < self.created_at
        {
            return Err(QuickRaidError::InvalidContract("timestamps"));
        }
        self.authority.validate()?;
        self.eligibility.validate()?;
        self.evidence_card.validate()?;
        if self.paper_bundle.is_some() != (self.stage == QuickRaidStageV1::Completed) {
            return Err(QuickRaidError::InvalidContract(
                "paper bundle stage binding",
            ));
        }
        if let Some(run) = &self.experiment_run {
            run.validate()?;
            if self.experiment_choice != Some(run.choice) {
                return Err(QuickRaidError::InvalidContract("experiment choice binding"));
            }
        }
        if let Some(bundle) = &self.paper_bundle {
            bundle.validate()?;
            if bundle.session_id != self.session_id
                || self.experiment_run.as_ref() != Some(&bundle.experiment_run)
                || self.conclusion != Some(bundle.conclusion)
            {
                return Err(QuickRaidError::InvalidContract("bundle binding"));
            }
        }
        let shape_valid = match self.stage {
            QuickRaidStageV1::EvidenceReview => {
                self.evidence_choice.is_none()
                    && self.experiment_choice.is_none()
                    && self.experiment_run.is_none()
                    && self.conclusion.is_none()
            }
            QuickRaidStageV1::ExperimentRun => {
                self.evidence_choice.is_some()
                    && self.experiment_choice.is_none()
                    && self.experiment_run.is_none()
                    && self.conclusion.is_none()
            }
            QuickRaidStageV1::PaperBundleReady => {
                self.evidence_choice.is_some()
                    && self.experiment_choice.is_some()
                    && self.experiment_run.is_some()
                    && self.conclusion.is_none()
            }
            QuickRaidStageV1::Completed => {
                self.evidence_choice.is_some()
                    && self.experiment_choice.is_some()
                    && self.experiment_run.is_some()
                    && self.conclusion.is_some()
                    && self.paper_bundle.is_some()
            }
            QuickRaidStageV1::Abandoned | QuickRaidStageV1::Expired => true,
        };
        if !shape_valid {
            return Err(QuickRaidError::InvalidContract("stage shape"));
        }
        if self.stage.is_terminal() {
            let expected_reason = match self.stage {
                QuickRaidStageV1::Completed => QuickRaidTerminalReasonV1::Completed,
                QuickRaidStageV1::Abandoned => QuickRaidTerminalReasonV1::PlayerAbandoned,
                QuickRaidStageV1::Expired => QuickRaidTerminalReasonV1::Expired,
                _ => unreachable!(),
            };
            if self.terminal_at != Some(self.updated_at)
                || self.terminal_reason != Some(expected_reason)
            {
                return Err(QuickRaidError::InvalidContract("terminal binding"));
            }
        } else if self.terminal_at.is_some() || self.terminal_reason.is_some() {
            return Err(QuickRaidError::InvalidContract("active terminal binding"));
        }
        Ok(())
    }

    pub fn apply_action(
        &mut self,
        request: &QuickRaidActionRequestV1,
        now: DateTime<Utc>,
    ) -> Result<QuickRaidEventV1, QuickRaidError> {
        self.validate()?;
        validate_action_request(request)?;
        if request.session_id != self.session_id
            || request.subject_id != self.subject_id
            || request.player_id != self.player_id
            || request.binding_id != self.binding_id
        {
            return Err(QuickRaidError::OwnerMismatch);
        }
        if request.expected_version != self.version {
            return Err(QuickRaidError::StaleVersion);
        }
        self.ensure_mutable(now)?;
        let from_version = self.version;
        let (event_kind, choice_code, result_hash) = match request.action {
            QuickRaidBrowserActionV1::ReviewEvidence { choice }
                if self.stage == QuickRaidStageV1::EvidenceReview =>
            {
                self.evidence_choice = Some(choice);
                self.stage = QuickRaidStageV1::ExperimentRun;
                (
                    QuickRaidEventKindV1::EvidenceReviewed,
                    Some(choice.code()),
                    None,
                )
            }
            QuickRaidBrowserActionV1::RunExperiment { choice }
                if self.stage == QuickRaidStageV1::ExperimentRun =>
            {
                self.experiment_choice = Some(choice);
                let run = QuickRaidExperimentRunV1::deterministic(Uuid::new_v4(), choice);
                let hash = run.run_hash.clone();
                self.experiment_run = Some(run);
                self.stage = QuickRaidStageV1::PaperBundleReady;
                (
                    QuickRaidEventKindV1::ExperimentRun,
                    Some(choice.code()),
                    Some(hash),
                )
            }
            QuickRaidBrowserActionV1::PublishPaper { conclusion }
                if self.stage == QuickRaidStageV1::PaperBundleReady =>
            {
                self.conclusion = Some(conclusion);
                let run = self
                    .experiment_run
                    .clone()
                    .ok_or(QuickRaidError::InvalidContract("experiment run"))?;
                self.paper_bundle = Some(QuickRaidPaperBundleV1::build(
                    self.authority.clone(),
                    self.session_id,
                    run,
                    conclusion,
                )?);
                self.stage = QuickRaidStageV1::Completed;
                self.terminal_at = Some(now);
                self.terminal_reason = Some(QuickRaidTerminalReasonV1::Completed);
                (
                    QuickRaidEventKindV1::PaperPublished,
                    Some(conclusion.code()),
                    self.paper_bundle
                        .as_ref()
                        .map(|bundle| bundle.paper_bundle_hash.clone()),
                )
            }
            QuickRaidBrowserActionV1::Abandon if !self.stage.is_terminal() => {
                self.stage = QuickRaidStageV1::Abandoned;
                self.terminal_at = Some(now);
                self.terminal_reason = Some(QuickRaidTerminalReasonV1::PlayerAbandoned);
                (QuickRaidEventKindV1::Abandoned, None, None)
            }
            _ => return Err(QuickRaidError::InvalidTransition),
        };
        self.advance(now)?;
        self.validate()?;
        Ok(QuickRaidEventV1 {
            schema: QUICK_RAID_EVENT_V1.to_owned(),
            event_id: request.event_id,
            session_id: self.session_id,
            actor_kind: QuickRaidActorKindV1::Browser,
            event_kind,
            from_version,
            to_version: self.version,
            request_hash: request.request_hash.clone(),
            choice_code: choice_code.map(str::to_owned),
            result_hash,
            occurred_at: now,
        })
    }

    pub fn expire(
        &mut self,
        event_id: Uuid,
        request_hash: String,
        now: DateTime<Utc>,
    ) -> Result<QuickRaidEventV1, QuickRaidError> {
        self.validate()?;
        if self.stage.is_terminal() {
            return Err(QuickRaidError::Terminal);
        }
        if now < self.expires_at {
            return Err(QuickRaidError::NotExpired);
        }
        if !valid_digest(&request_hash).is_ok() {
            return Err(QuickRaidError::InvalidContract("request hash"));
        }
        let from_version = self.version;
        self.stage = QuickRaidStageV1::Expired;
        self.terminal_at = Some(now);
        self.terminal_reason = Some(QuickRaidTerminalReasonV1::Expired);
        self.advance(now)?;
        self.validate()?;
        Ok(QuickRaidEventV1 {
            schema: QUICK_RAID_EVENT_V1.to_owned(),
            event_id,
            session_id: self.session_id,
            actor_kind: QuickRaidActorKindV1::Server,
            event_kind: QuickRaidEventKindV1::Expired,
            from_version,
            to_version: self.version,
            request_hash,
            choice_code: None,
            result_hash: None,
            occurred_at: now,
        })
    }

    fn ensure_mutable(&self, now: DateTime<Utc>) -> Result<(), QuickRaidError> {
        if self.stage.is_terminal() {
            return Err(QuickRaidError::Terminal);
        }
        if now >= self.expires_at {
            return Err(QuickRaidError::Expired);
        }
        if now < self.updated_at {
            return Err(QuickRaidError::InvalidContract("non-monotonic time"));
        }
        if self.version >= JSON_SAFE_U64_MAX {
            return Err(QuickRaidError::VersionExhausted);
        }
        Ok(())
    }

    fn advance(&mut self, now: DateTime<Utc>) -> Result<(), QuickRaidError> {
        self.version = self
            .version
            .checked_add(1)
            .filter(|version| *version <= JSON_SAFE_U64_MAX)
            .ok_or(QuickRaidError::VersionExhausted)?;
        self.updated_at = now;
        Ok(())
    }
}

fn validate_action_request(request: &QuickRaidActionRequestV1) -> Result<(), QuickRaidError> {
    if request.schema != QUICK_RAID_ACTION_V1
        || request.event_id.is_nil()
        || request.session_id.is_nil()
        || request.player_id.is_nil()
        || request.binding_id.is_nil()
        || request.expected_version == 0
        || request.expected_version > JSON_SAFE_U64_MAX
        || request.subject_id.is_empty()
        || request.subject_id.trim() != request.subject_id
    {
        return Err(QuickRaidError::InvalidContract("action envelope"));
    }
    valid_digest(&request.request_hash)
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum QuickRaidError {
    #[error("invalid Quick Raid contract: {0}")]
    InvalidContract(&'static str),
    #[error("Quick Raid authority escaped its non-economic boundary")]
    AuthorityEscape,
    #[error("Quick Raid challenge is not open or does not match the fixed pack")]
    ChallengeUnavailable,
    #[error("Quick Raid session owner mismatch")]
    OwnerMismatch,
    #[error("Quick Raid request version is stale")]
    StaleVersion,
    #[error("Quick Raid session has expired")]
    Expired,
    #[error("Quick Raid session is not yet expired")]
    NotExpired,
    #[error("Quick Raid session is terminal")]
    Terminal,
    #[error("invalid Quick Raid transition")]
    InvalidTransition,
    #[error("Quick Raid version exhausted")]
    VersionExhausted,
}

fn canonical_uuid(value: Option<&serde_json::Value>) -> Result<Uuid, QuickRaidError> {
    let text = value
        .and_then(serde_json::Value::as_str)
        .ok_or(QuickRaidError::ChallengeUnavailable)?;
    let parsed = Uuid::parse_str(text).map_err(|_| QuickRaidError::ChallengeUnavailable)?;
    if parsed.is_nil() || parsed.to_string() != text {
        return Err(QuickRaidError::ChallengeUnavailable);
    }
    Ok(parsed)
}

fn canonical_uuid_text(value: Uuid) -> Result<Uuid, QuickRaidError> {
    if value.is_nil() {
        Err(QuickRaidError::InvalidContract("nil UUID"))
    } else {
        Ok(value)
    }
}

fn digest(value: Option<&serde_json::Value>) -> Result<String, QuickRaidError> {
    let text = value
        .and_then(serde_json::Value::as_str)
        .ok_or(QuickRaidError::ChallengeUnavailable)?
        .to_owned();
    valid_digest(&text)?;
    Ok(text)
}

fn valid_digest(value: &str) -> Result<(), QuickRaidError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(QuickRaidError::InvalidContract("digest"));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(QuickRaidError::InvalidContract("digest"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn authority() -> QuickRaidAuthorityV1 {
        let value = json!({
            "challenge_id":"11111111-1111-4111-8111-111111111111",
            "activation_id":"22222222-2222-4222-8222-222222222222",
            "activation_request_sha256":format!("sha256:{}", "a".repeat(64)),
            "ruleset_hash":format!("sha256:{}", "b".repeat(64)),
            "ruleset_enforcement":"authoritative_v1",
            "status":"open",
            "ruleset":{
                "template":"evidence-audit",
                "duration_seconds":900,
                "gameplay":{"modifiers":["quick-raid-fixed-seed"]}
            },
            "description":format!("pack_id={QUICK_RAID_PACK_ID}"),
            "ruleset_version":QUICK_RAID_RULESET_VERSION
        });
        QuickRaidAuthorityV1::from_catalog(value.as_object().map(|_| &value).unwrap()).unwrap()
    }

    #[test]
    fn fixed_authority_rejects_wrong_duration_or_modifier() {
        let mut challenge = json!({
            "challenge_id":"11111111-1111-4111-8111-111111111111",
            "activation_id":"22222222-2222-4222-8222-222222222222",
            "activation_request_sha256":format!("sha256:{}", "a".repeat(64)),
            "ruleset_hash":format!("sha256:{}", "b".repeat(64)),
            "ruleset_enforcement":"authoritative_v1",
            "status":"open",
            "ruleset":{"template":"evidence-audit","duration_seconds":2700,"gameplay":{"modifiers":["quick-raid-fixed-seed"]}},
            "description":format!("pack_id={QUICK_RAID_PACK_ID}"),
            "ruleset_version":QUICK_RAID_RULESET_VERSION
        });
        assert_eq!(
            QuickRaidAuthorityV1::from_catalog(&challenge),
            Err(QuickRaidError::ChallengeUnavailable)
        );
        challenge["ruleset"]["duration_seconds"] = json!(900);
        challenge["ruleset"]["gameplay"]["modifiers"] = json!(["other"]);
        assert_eq!(
            QuickRaidAuthorityV1::from_catalog(&challenge),
            Err(QuickRaidError::ChallengeUnavailable)
        );
    }

    #[test]
    fn fixed_seed_flow_emits_one_bundle_and_locks_authority() {
        let now = Utc::now();
        let session_id = Uuid::new_v4();
        let mut session = QuickRaidSessionV1::new(
            session_id,
            "quick-player".into(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            authority(),
            now,
        )
        .unwrap();
        let owner = (
            session.subject_id.clone(),
            session.player_id,
            session.binding_id,
        );
        let action = |version, action| QuickRaidActionRequestV1 {
            schema: QUICK_RAID_ACTION_V1.into(),
            event_id: Uuid::new_v4(),
            session_id,
            subject_id: owner.0.clone(),
            player_id: owner.1,
            binding_id: owner.2,
            expected_version: version,
            request_hash: format!("sha256:{}", "c".repeat(64)),
            action,
        };
        session
            .apply_action(
                &action(
                    1,
                    QuickRaidBrowserActionV1::ReviewEvidence {
                        choice: QuickRaidEvidenceChoiceV1::FlagCitationGap,
                    },
                ),
                now,
            )
            .unwrap();
        session
            .apply_action(
                &action(
                    2,
                    QuickRaidBrowserActionV1::RunExperiment {
                        choice: QuickRaidExperimentChoiceV1::RunCandidate,
                    },
                ),
                now,
            )
            .unwrap();
        session
            .apply_action(
                &action(
                    3,
                    QuickRaidBrowserActionV1::PublishPaper {
                        conclusion: QuickRaidConclusionV1::RetainWithCaveat,
                    },
                ),
                now,
            )
            .unwrap();
        let bundle = session.paper_bundle.as_ref().unwrap();
        assert_eq!(bundle.seed, QUICK_RAID_FIXED_SEED);
        assert_eq!(bundle.evidence_card.seed, QUICK_RAID_FIXED_SEED);
        assert_eq!(bundle.experiment_run.seed, QUICK_RAID_FIXED_SEED);
        assert!(!bundle.portable);
        assert_eq!(bundle.finality, "none");
        assert!(!session.eligibility.ranking_eligible);
        assert!(!session.eligibility.economic_eligible);
    }

    #[test]
    fn stale_and_wrong_owner_actions_fail_closed() {
        let now = Utc::now();
        let mut session = QuickRaidSessionV1::new(
            Uuid::new_v4(),
            "quick-player".into(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            authority(),
            now,
        )
        .unwrap();
        let request = QuickRaidActionRequestV1 {
            schema: QUICK_RAID_ACTION_V1.into(),
            event_id: Uuid::new_v4(),
            session_id: session.session_id,
            subject_id: "other".into(),
            player_id: session.player_id,
            binding_id: session.binding_id,
            expected_version: 1,
            request_hash: format!("sha256:{}", "d".repeat(64)),
            action: QuickRaidBrowserActionV1::ReviewEvidence {
                choice: QuickRaidEvidenceChoiceV1::FlagCitationGap,
            },
        };
        assert_eq!(
            session.apply_action(&request, now),
            Err(QuickRaidError::OwnerMismatch)
        );
    }
}
