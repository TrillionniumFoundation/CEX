use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::paper_raid_contracts::{canonical_json_sha256, decode_digest};

pub const CHALLENGE_RULESET_V1: &str = "hepta.challenge.ruleset.v1";
pub const CHALLENGE_RULESET_SNAPSHOT_V1: &str = "hepta.paper_raid.challenge_ruleset_snapshot.v1";

const MIN_DURATION_SECONDS: u32 = 15 * 60;
const MAX_DURATION_SECONDS: u32 = 7 * 24 * 60 * 60;
const MAX_GRACE_SECONDS: u32 = 24 * 60 * 60;
const MAX_REQUIREMENT_MINIMUM: u16 = 32;
const MAX_GAMEPLAY_TEXT_CHARS: usize = 512;
const MAX_GAMEPLAY_MODIFIERS: usize = 8;
const MAX_GAMEPLAY_MODIFIER_CHARS: usize = 48;
const MAX_ROLE_RESOURCE_UNITS: u16 = 32;
const MAX_FAILURE_FOCUS_REFUND: u16 = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ChallengeTemplateV1 {
    BenchmarkAblation,
    Replication,
    EvidenceAudit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeForwardTransitionV1 {
    PreregisteringToResearching,
    ResearchingToExperimenting,
    ExperimentingToDrafting,
    DraftingToIntegrityReview,
    IntegrityReviewToReproducing,
    ReproducingToAuthorApproval,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRequirementKindV1 {
    WorkItems,
    AcceptedWorkItems,
    ArtifactManifests,
    EvidenceCards,
    Citations,
    ExperimentPlans,
    RetainedRuns,
    SuccessfulRuns,
    RetainedFailedRuns,
    Claims,
    SectionRevisions,
    ApprovingSectionReviews,
    SectionMerges,
    PaperRevisions,
    AllWorkItemsTerminal,
    PaperRevisionCoversSectionMerges,
    ReleaseCandidate,
    AllAuthorConsents,
}

impl ChallengeRequirementKindV1 {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::WorkItems => "work_items",
            Self::AcceptedWorkItems => "accepted_work_items",
            Self::ArtifactManifests => "artifact_manifests",
            Self::EvidenceCards => "evidence_cards",
            Self::Citations => "citations",
            Self::ExperimentPlans => "experiment_plans",
            Self::RetainedRuns => "retained_runs",
            Self::SuccessfulRuns => "successful_runs",
            Self::RetainedFailedRuns => "retained_failed_runs",
            Self::Claims => "claims",
            Self::SectionRevisions => "section_revisions",
            Self::ApprovingSectionReviews => "approving_section_reviews",
            Self::SectionMerges => "section_merges",
            Self::PaperRevisions => "paper_revisions",
            Self::AllWorkItemsTerminal => "all_work_items_terminal",
            Self::PaperRevisionCoversSectionMerges => "paper_revision_covers_section_merges",
            Self::ReleaseCandidate => "release_candidate",
            Self::AllAuthorConsents => "all_author_consents",
        }
    }

    fn is_boolean(self) -> bool {
        matches!(
            self,
            Self::AllWorkItemsTerminal
                | Self::PaperRevisionCoversSectionMerges
                | Self::ReleaseCandidate
                | Self::AllAuthorConsents
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeMinimumV1 {
    pub kind: ChallengeRequirementKindV1,
    pub minimum: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengePhaseGateV1 {
    pub transition: ChallengeForwardTransitionV1,
    pub requirements: Vec<ChallengeMinimumV1>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeDifficultyV1 {
    Introductory,
    Intermediate,
    Advanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeRoleResourcesV1 {
    pub captain_focus: u16,
    pub evidence_focus: u16,
    pub experiment_focus: u16,
    pub run_budget: u16,
    pub retained_failure_focus_refund: u16,
}

impl ChallengeRoleResourcesV1 {
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("captain_focus", self.captain_focus),
            ("evidence_focus", self.evidence_focus),
            ("experiment_focus", self.experiment_focus),
            ("run_budget", self.run_budget),
        ] {
            if !(1..=MAX_ROLE_RESOURCE_UNITS).contains(&value) {
                return Err(format!(
                    "gameplay.role_resources.{field} must be between 1 and {MAX_ROLE_RESOURCE_UNITS}"
                ));
            }
        }
        if self.retained_failure_focus_refund == 0
            || self.retained_failure_focus_refund > MAX_FAILURE_FOCUS_REFUND
            || self.retained_failure_focus_refund > self.experiment_focus
        {
            return Err(format!(
                "gameplay.role_resources.retained_failure_focus_refund must be between 1 and {MAX_FAILURE_FOCUS_REFUND} and not exceed experiment_focus"
            ));
        }
        if self.captain_focus > self.evidence_focus
            || self.captain_focus > self.experiment_focus
            || self.captain_focus > self.run_budget
        {
            return Err(
                "gameplay.role_resources captain_focus must be supportable by evidence, experiment, and run allocations"
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeGameplayV1 {
    pub difficulty: ChallengeDifficultyV1,
    pub objective: String,
    pub risk: String,
    pub modifiers: Vec<String>,
    pub victory_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_resources: Option<ChallengeRoleResourcesV1>,
}

impl ChallengeGameplayV1 {
    pub fn validate(&self) -> Result<(), String> {
        validate_gameplay_text("gameplay.objective", &self.objective)?;
        validate_gameplay_text("gameplay.risk", &self.risk)?;
        validate_gameplay_text("gameplay.victory_summary", &self.victory_summary)?;
        if let Some(role_resources) = &self.role_resources {
            role_resources.validate()?;
        }
        if self.modifiers.len() > MAX_GAMEPLAY_MODIFIERS {
            return Err(format!(
                "gameplay.modifiers must contain at most {MAX_GAMEPLAY_MODIFIERS} entries"
            ));
        }
        let mut previous: Option<&str> = None;
        for modifier in &self.modifiers {
            validate_gameplay_modifier(modifier)?;
            if let Some(previous) = previous {
                if previous == modifier {
                    return Err(format!(
                        "gameplay.modifiers contains duplicate modifier {modifier}"
                    ));
                }
                if previous > modifier.as_str() {
                    return Err(
                        "gameplay.modifiers must use canonical ascending lexical order".into(),
                    );
                }
            }
            previous = Some(modifier);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ChallengeRulesetV1 {
    pub schema: String,
    pub template: ChallengeTemplateV1,
    pub duration_seconds: u32,
    pub grace_seconds: u32,
    pub phase_gates: Vec<ChallengePhaseGateV1>,
    pub victory_requirements: Vec<ChallengeMinimumV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gameplay: Option<ChallengeGameplayV1>,
}

impl ChallengeRulesetV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != CHALLENGE_RULESET_V1 {
            return Err(format!("schema must equal {CHALLENGE_RULESET_V1}"));
        }
        if !(MIN_DURATION_SECONDS..=MAX_DURATION_SECONDS).contains(&self.duration_seconds) {
            return Err(format!(
                "duration_seconds must be between {MIN_DURATION_SECONDS} and {MAX_DURATION_SECONDS}"
            ));
        }
        if self.grace_seconds > MAX_GRACE_SECONDS {
            return Err(format!("grace_seconds must not exceed {MAX_GRACE_SECONDS}"));
        }
        let expected_transitions = [
            ChallengeForwardTransitionV1::PreregisteringToResearching,
            ChallengeForwardTransitionV1::ResearchingToExperimenting,
            ChallengeForwardTransitionV1::ExperimentingToDrafting,
            ChallengeForwardTransitionV1::DraftingToIntegrityReview,
            ChallengeForwardTransitionV1::IntegrityReviewToReproducing,
            ChallengeForwardTransitionV1::ReproducingToAuthorApproval,
        ];
        if self.phase_gates.len() != expected_transitions.len() {
            return Err("phase_gates must contain every forward transition exactly once".into());
        }
        let mut transitions = HashSet::new();
        for gate in &self.phase_gates {
            if !transitions.insert(gate.transition) {
                return Err(format!("duplicate phase gate for {:?}", gate.transition));
            }
            validate_minimums("phase gate", &gate.requirements)?;
            for requirement in &gate.requirements {
                if !requirement_allowed_at(gate.transition, requirement.kind) {
                    return Err(format!(
                        "{} is not observable at {:?}",
                        requirement.kind.code(),
                        gate.transition
                    ));
                }
            }
        }
        if expected_transitions
            .iter()
            .any(|transition| !transitions.contains(transition))
        {
            return Err("phase_gates must contain every forward transition exactly once".into());
        }
        validate_minimums("victory", &self.victory_requirements)?;
        if let Some(gameplay) = &self.gameplay {
            gameplay.validate()?;
        }
        self.validate_template_floors()?;
        self.validate_role_resource_reachability()
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        self.validate()?;
        canonical_json_sha256(self)
    }

    pub(crate) fn requirements_for(
        &self,
        transition: ChallengeForwardTransitionV1,
    ) -> &[ChallengeMinimumV1] {
        self.phase_gates
            .iter()
            .find(|gate| gate.transition == transition)
            .map(|gate| gate.requirements.as_slice())
            .unwrap_or(&[])
    }

    fn phase_minimum_for(
        &self,
        transition: ChallengeForwardTransitionV1,
        kind: ChallengeRequirementKindV1,
    ) -> u16 {
        self.phase_gates
            .iter()
            .find(|gate| gate.transition == transition)
            .and_then(|gate| {
                gate.requirements
                    .iter()
                    .find(|requirement| requirement.kind == kind)
            })
            .map(|requirement| requirement.minimum)
            .unwrap_or_default()
    }

    fn victory_minimum_for(&self, kind: ChallengeRequirementKindV1) -> u16 {
        self.victory_requirements
            .iter()
            .find(|requirement| requirement.kind == kind)
            .map(|requirement| requirement.minimum)
            .unwrap_or_default()
    }

    fn maximum_run_minimum(&self, kind: ChallengeRequirementKindV1) -> u16 {
        self.phase_gates
            .iter()
            .flat_map(|gate| gate.requirements.iter())
            .chain(self.victory_requirements.iter())
            .filter(|requirement| requirement.kind == kind)
            .map(|requirement| requirement.minimum)
            .max()
            .unwrap_or_default()
    }

    fn validate_role_resource_reachability(&self) -> Result<(), String> {
        use ChallengeRequirementKindV1 as Requirement;

        let Some(role_resources) = self
            .gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.role_resources.as_ref())
        else {
            return Ok(());
        };
        let retained_runs = u32::from(self.maximum_run_minimum(Requirement::RetainedRuns));
        let successful_runs = u32::from(self.maximum_run_minimum(Requirement::SuccessfulRuns));
        let retained_failed_runs =
            u32::from(self.maximum_run_minimum(Requirement::RetainedFailedRuns));
        // Successful and retained-failed runs are disjoint records. A generic
        // retained-run minimum may overlap both, so the hard lower bound is the
        // larger of that minimum and their sum.
        let required_run_budget =
            retained_runs.max(successful_runs.saturating_add(retained_failed_runs));
        if u32::from(role_resources.run_budget) < required_run_budget {
            return Err(format!(
                "gameplay.role_resources.run_budget must be at least {required_run_budget} to satisfy the hard retained/successful/retained-failed run minima"
            ));
        }
        // A retained failed run refunds the focus it spent, but successful
        // runs never do. Scheduling retained failures first is therefore
        // reachable only when focus covers every required successful run.
        if u32::from(role_resources.experiment_focus) < successful_runs {
            return Err(format!(
                "gameplay.role_resources.experiment_focus must be at least {successful_runs} to satisfy the hard successful run minimum"
            ));
        }
        Ok(())
    }

    fn validate_template_floors(&self) -> Result<(), String> {
        use ChallengeForwardTransitionV1 as Transition;
        use ChallengeRequirementKindV1 as Requirement;

        let evidence_minimum = if self.template == ChallengeTemplateV1::EvidenceAudit {
            2
        } else {
            1
        };
        let common_phase_floors = [
            (
                Transition::PreregisteringToResearching,
                Requirement::WorkItems,
                1,
            ),
            (
                Transition::PreregisteringToResearching,
                Requirement::ArtifactManifests,
                1,
            ),
            (
                Transition::ResearchingToExperimenting,
                Requirement::EvidenceCards,
                evidence_minimum,
            ),
            (
                Transition::ResearchingToExperimenting,
                Requirement::Citations,
                evidence_minimum,
            ),
            (
                Transition::ResearchingToExperimenting,
                Requirement::Claims,
                evidence_minimum,
            ),
            (
                Transition::DraftingToIntegrityReview,
                Requirement::AllWorkItemsTerminal,
                1,
            ),
            (
                Transition::DraftingToIntegrityReview,
                Requirement::SectionRevisions,
                1,
            ),
            (
                Transition::DraftingToIntegrityReview,
                Requirement::PaperRevisions,
                1,
            ),
            (
                Transition::IntegrityReviewToReproducing,
                Requirement::ApprovingSectionReviews,
                1,
            ),
            (
                Transition::IntegrityReviewToReproducing,
                Requirement::SectionMerges,
                1,
            ),
            (
                Transition::ReproducingToAuthorApproval,
                Requirement::PaperRevisionCoversSectionMerges,
                1,
            ),
        ];
        for (transition, kind, minimum) in common_phase_floors {
            if self.phase_minimum_for(transition, kind) < minimum {
                return Err(format!(
                    "{:?} template must require at least {minimum} {} at {:?}",
                    self.template,
                    kind.code(),
                    transition
                ));
            }
        }
        let template_requirements: &[Requirement] = match self.template {
            ChallengeTemplateV1::BenchmarkAblation => &[
                Requirement::ExperimentPlans,
                Requirement::SuccessfulRuns,
                Requirement::RetainedFailedRuns,
            ],
            ChallengeTemplateV1::Replication => {
                &[Requirement::ExperimentPlans, Requirement::SuccessfulRuns]
            }
            ChallengeTemplateV1::EvidenceAudit => &[],
        };
        if let Some(missing) = template_requirements
            .iter()
            .find(|kind| self.phase_minimum_for(Transition::ExperimentingToDrafting, **kind) == 0)
        {
            return Err(format!(
                "{:?} template must require at least one {} at {:?}",
                self.template,
                missing.code(),
                Transition::ExperimentingToDrafting
            ));
        }
        if self.victory_minimum_for(Requirement::AcceptedWorkItems) == 0 {
            return Err(
                "victory_requirements must require at least one accepted_work_items".into(),
            );
        }
        for required_victory in [
            Requirement::ReleaseCandidate,
            Requirement::AllAuthorConsents,
            Requirement::PaperRevisionCoversSectionMerges,
        ] {
            if self.victory_minimum_for(required_victory) != 1 {
                return Err(format!(
                    "victory_requirements must require {} exactly once",
                    required_victory.code()
                ));
            }
        }
        Ok(())
    }
}

fn validate_gameplay_text(field: &str, value: &str) -> Result<(), String> {
    let character_count = value.chars().count();
    if character_count == 0 || character_count > MAX_GAMEPLAY_TEXT_CHARS {
        return Err(format!(
            "{field} must contain between 1 and {MAX_GAMEPLAY_TEXT_CHARS} characters"
        ));
    }
    if value.trim() != value {
        return Err(format!("{field} must not contain surrounding whitespace"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}

fn validate_gameplay_modifier(value: &str) -> Result<(), String> {
    let character_count = value.chars().count();
    if character_count == 0 || character_count > MAX_GAMEPLAY_MODIFIER_CHARS {
        return Err(format!(
            "gameplay modifier must contain between 1 and {MAX_GAMEPLAY_MODIFIER_CHARS} characters"
        ));
    }
    let bytes = value.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_lowercase)
        || !bytes
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || bytes
            .iter()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'))
        || bytes.windows(2).any(|pair| pair == b"--")
    {
        return Err(format!(
            "gameplay modifier {value} must be canonical lowercase kebab-case"
        ));
    }
    Ok(())
}

fn requirement_allowed_at(
    transition: ChallengeForwardTransitionV1,
    requirement: ChallengeRequirementKindV1,
) -> bool {
    use ChallengeForwardTransitionV1 as Transition;
    use ChallengeRequirementKindV1 as Requirement;

    match transition {
        Transition::PreregisteringToResearching => matches!(
            requirement,
            Requirement::WorkItems
                | Requirement::AcceptedWorkItems
                | Requirement::ArtifactManifests
                | Requirement::ExperimentPlans
                | Requirement::AllWorkItemsTerminal
        ),
        Transition::ResearchingToExperimenting => matches!(
            requirement,
            Requirement::ArtifactManifests
                | Requirement::EvidenceCards
                | Requirement::Citations
                | Requirement::ExperimentPlans
                | Requirement::Claims
        ),
        Transition::ExperimentingToDrafting => matches!(
            requirement,
            Requirement::ArtifactManifests
                | Requirement::ExperimentPlans
                | Requirement::RetainedRuns
                | Requirement::SuccessfulRuns
                | Requirement::RetainedFailedRuns
        ),
        Transition::DraftingToIntegrityReview => matches!(
            requirement,
            Requirement::WorkItems
                | Requirement::AcceptedWorkItems
                | Requirement::ArtifactManifests
                | Requirement::SectionRevisions
                | Requirement::PaperRevisions
                | Requirement::AllWorkItemsTerminal
        ),
        Transition::IntegrityReviewToReproducing => matches!(
            requirement,
            Requirement::ApprovingSectionReviews | Requirement::SectionMerges
        ),
        Transition::ReproducingToAuthorApproval => matches!(
            requirement,
            Requirement::PaperRevisions | Requirement::PaperRevisionCoversSectionMerges
        ),
    }
}

fn validate_minimums(scope: &str, requirements: &[ChallengeMinimumV1]) -> Result<(), String> {
    let mut kinds = HashSet::new();
    for requirement in requirements {
        if !kinds.insert(requirement.kind) {
            return Err(format!(
                "{scope} contains duplicate {} requirement",
                requirement.kind.code()
            ));
        }
        if requirement.minimum == 0 || requirement.minimum > MAX_REQUIREMENT_MINIMUM {
            return Err(format!(
                "{} minimum must be between 1 and {MAX_REQUIREMENT_MINIMUM}",
                requirement.kind.code()
            ));
        }
        if requirement.kind.is_boolean() && requirement.minimum != 1 {
            return Err(format!(
                "{} is boolean and must have minimum 1",
                requirement.kind.code()
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeRulesetEnforcementV1 {
    AuthoritativeV1,
    LegacyUnranked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaperChallengeRulesetSnapshotV1 {
    pub schema: String,
    pub challenge_snapshot_hash: String,
    pub ruleset_version: String,
    pub ruleset_hash: String,
    pub enforcement: ChallengeRulesetEnforcementV1,
    pub ruleset: Option<ChallengeRulesetV1>,
}

impl PaperChallengeRulesetSnapshotV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != CHALLENGE_RULESET_SNAPSHOT_V1 {
            return Err(format!(
                "snapshot schema must equal {CHALLENGE_RULESET_SNAPSHOT_V1}"
            ));
        }
        decode_digest(&self.challenge_snapshot_hash)
            .map_err(|message| format!("invalid challenge_snapshot_hash: {message}"))?;
        // The historical Challenge API accepted uppercase SHA-256 hex. Keep
        // that legacy snapshot readable; authoritative typed hashes are still
        // compared byte-for-byte to the canonical lowercase computed value.
        decode_digest(&self.ruleset_hash.to_ascii_lowercase())
            .map_err(|message| format!("invalid ruleset_hash: {message}"))?;
        match self.enforcement {
            ChallengeRulesetEnforcementV1::LegacyUnranked => {
                if self.ruleset.is_some() {
                    return Err(
                        "legacy-unranked challenge snapshot must not contain a typed ruleset"
                            .into(),
                    );
                }
            }
            ChallengeRulesetEnforcementV1::AuthoritativeV1 => {
                let ruleset = self.ruleset.as_ref().ok_or_else(|| {
                    "authoritative challenge snapshot must contain a typed ruleset".to_string()
                })?;
                let ruleset_hash = ruleset.canonical_hash()?;
                if ruleset_hash != self.ruleset_hash {
                    return Err(
                        "authoritative challenge snapshot ruleset_hash does not match its typed ruleset"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        self.validate()?;
        canonical_json_sha256(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requirement(kind: ChallengeRequirementKindV1) -> ChallengeMinimumV1 {
        minimum(kind, 1)
    }

    fn minimum(kind: ChallengeRequirementKindV1, value: u16) -> ChallengeMinimumV1 {
        ChallengeMinimumV1 {
            kind,
            minimum: value,
        }
    }

    fn valid_ruleset(template: ChallengeTemplateV1) -> ChallengeRulesetV1 {
        use ChallengeForwardTransitionV1 as Transition;
        use ChallengeRequirementKindV1 as Requirement;

        let mut experiment = vec![requirement(Requirement::ArtifactManifests)];
        if template != ChallengeTemplateV1::EvidenceAudit {
            experiment.extend([
                requirement(Requirement::ExperimentPlans),
                requirement(Requirement::SuccessfulRuns),
            ]);
        }
        if template == ChallengeTemplateV1::BenchmarkAblation {
            experiment.push(requirement(Requirement::RetainedFailedRuns));
        }
        ChallengeRulesetV1 {
            schema: CHALLENGE_RULESET_V1.into(),
            template,
            duration_seconds: 5_400,
            grace_seconds: 900,
            phase_gates: vec![
                ChallengePhaseGateV1 {
                    transition: Transition::PreregisteringToResearching,
                    requirements: vec![
                        requirement(Requirement::WorkItems),
                        requirement(Requirement::ArtifactManifests),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ResearchingToExperimenting,
                    requirements: vec![
                        minimum(
                            Requirement::EvidenceCards,
                            if template == ChallengeTemplateV1::EvidenceAudit {
                                2
                            } else {
                                1
                            },
                        ),
                        minimum(
                            Requirement::Citations,
                            if template == ChallengeTemplateV1::EvidenceAudit {
                                2
                            } else {
                                1
                            },
                        ),
                        minimum(
                            Requirement::Claims,
                            if template == ChallengeTemplateV1::EvidenceAudit {
                                2
                            } else {
                                1
                            },
                        ),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ExperimentingToDrafting,
                    requirements: experiment,
                },
                ChallengePhaseGateV1 {
                    transition: Transition::DraftingToIntegrityReview,
                    requirements: vec![
                        requirement(Requirement::AllWorkItemsTerminal),
                        requirement(Requirement::SectionRevisions),
                        requirement(Requirement::PaperRevisions),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::IntegrityReviewToReproducing,
                    requirements: vec![
                        requirement(Requirement::ApprovingSectionReviews),
                        requirement(Requirement::SectionMerges),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ReproducingToAuthorApproval,
                    requirements: vec![requirement(Requirement::PaperRevisionCoversSectionMerges)],
                },
            ],
            victory_requirements: vec![
                requirement(Requirement::AcceptedWorkItems),
                requirement(Requirement::ReleaseCandidate),
                requirement(Requirement::AllAuthorConsents),
                requirement(Requirement::PaperRevisionCoversSectionMerges),
            ],
            gameplay: None,
        }
    }

    fn typed_gameplay() -> ChallengeGameplayV1 {
        ChallengeGameplayV1 {
            difficulty: ChallengeDifficultyV1::Intermediate,
            objective: "Separate the ablation effect from measurement noise.".into(),
            risk: "A hidden confound can make the apparent effect non-causal.".into(),
            modifiers: vec!["failure-retention".into(), "frozen-evaluator".into()],
            victory_summary: "Pass every hard gate and retain all failed runs.".into(),
            role_resources: None,
        }
    }

    #[test]
    fn all_three_templates_are_valid_and_hash_deterministically() {
        for template in [
            ChallengeTemplateV1::BenchmarkAblation,
            ChallengeTemplateV1::Replication,
            ChallengeTemplateV1::EvidenceAudit,
        ] {
            let ruleset = valid_ruleset(template);
            ruleset.validate().expect("valid ruleset");
            let first = ruleset.canonical_hash().expect("ruleset hash");
            let second = ruleset.canonical_hash().expect("ruleset hash");
            assert_eq!(first, second);
            assert!(first.starts_with("sha256:"));
        }
    }

    #[test]
    fn absent_gameplay_preserves_legacy_canonical_bytes_and_hash() {
        #[derive(Serialize)]
        struct LegacyChallengeRulesetV1<'a> {
            schema: &'a str,
            template: ChallengeTemplateV1,
            duration_seconds: u32,
            grace_seconds: u32,
            phase_gates: &'a [ChallengePhaseGateV1],
            victory_requirements: &'a [ChallengeMinimumV1],
        }

        let ruleset = valid_ruleset(ChallengeTemplateV1::Replication);
        let legacy_shape = LegacyChallengeRulesetV1 {
            schema: &ruleset.schema,
            template: ruleset.template,
            duration_seconds: ruleset.duration_seconds,
            grace_seconds: ruleset.grace_seconds,
            phase_gates: &ruleset.phase_gates,
            victory_requirements: &ruleset.victory_requirements,
        };
        assert_eq!(
            serde_json::to_vec(&ruleset).expect("serialize extended ruleset"),
            serde_json::to_vec(&legacy_shape).expect("serialize legacy ruleset")
        );
        assert_eq!(
            ruleset.canonical_hash().expect("ruleset hash"),
            canonical_json_sha256(&legacy_shape).expect("legacy shape hash")
        );

        let serialized = serde_json::to_value(&legacy_shape).expect("legacy JSON");
        assert!(serialized.get("gameplay").is_none());
        let decoded: ChallengeRulesetV1 =
            serde_json::from_value(serialized).expect("deserialize legacy ruleset");
        assert!(decoded.gameplay.is_none());
        assert_eq!(decoded, ruleset);
    }

    #[test]
    fn typed_gameplay_is_validated_and_participates_in_the_canonical_hash() {
        let mut ruleset = valid_ruleset(ChallengeTemplateV1::BenchmarkAblation);
        let legacy_hash = ruleset.canonical_hash().expect("legacy hash");
        ruleset.gameplay = Some(typed_gameplay());
        ruleset.validate().expect("typed gameplay");
        let typed_hash = ruleset.canonical_hash().expect("typed hash");
        assert_ne!(legacy_hash, typed_hash);
        assert_eq!(
            serde_json::to_value(&ruleset).expect("serialize ruleset")["gameplay"]["difficulty"],
            "intermediate"
        );
    }

    #[test]
    fn optional_role_resources_preserve_old_typed_bytes_and_join_the_hash_when_present() {
        let gameplay = typed_gameplay();
        let legacy_bytes = serde_json::to_vec(&gameplay).expect("legacy typed gameplay bytes");
        assert!(!String::from_utf8(legacy_bytes.clone())
            .expect("JSON")
            .contains("role_resources"));

        let mut enabled = gameplay;
        enabled.role_resources = Some(ChallengeRoleResourcesV1 {
            captain_focus: 2,
            evidence_focus: 3,
            experiment_focus: 3,
            run_budget: 4,
            retained_failure_focus_refund: 1,
        });
        enabled.validate().expect("role resources");
        assert_ne!(
            canonical_json_sha256(&enabled).expect("resource gameplay hash"),
            canonical_json_sha256(
                &serde_json::from_slice::<ChallengeGameplayV1>(&legacy_bytes)
                    .expect("legacy typed gameplay")
            )
            .expect("legacy gameplay hash")
        );
    }

    #[test]
    fn role_resources_reject_impossible_or_economic_pressure_allocations() {
        let mut allocation = ChallengeRoleResourcesV1 {
            captain_focus: 3,
            evidence_focus: 2,
            experiment_focus: 3,
            run_budget: 3,
            retained_failure_focus_refund: 1,
        };
        assert!(allocation
            .validate()
            .unwrap_err()
            .contains("must be supportable"));
        allocation.captain_focus = 2;
        allocation.retained_failure_focus_refund = 5;
        assert!(allocation
            .validate()
            .unwrap_err()
            .contains("must be between"));
        allocation.retained_failure_focus_refund = 0;
        assert!(allocation
            .validate()
            .unwrap_err()
            .contains("must be between"));
    }

    #[test]
    fn role_run_budget_must_cover_disjoint_hard_run_minima() {
        let mut ruleset = valid_ruleset(ChallengeTemplateV1::BenchmarkAblation);
        let mut gameplay = typed_gameplay();
        gameplay.role_resources = Some(ChallengeRoleResourcesV1 {
            captain_focus: 1,
            evidence_focus: 1,
            experiment_focus: 1,
            run_budget: 1,
            retained_failure_focus_refund: 1,
        });
        ruleset.gameplay = Some(gameplay);

        let error = ruleset
            .validate()
            .expect_err("one run cannot be both successful and retained-failed");
        assert!(error.contains("run_budget must be at least 2"));

        ruleset
            .gameplay
            .as_mut()
            .and_then(|gameplay| gameplay.role_resources.as_mut())
            .expect("role resources")
            .run_budget = 2;
        ruleset.validate().expect("two-run budget is reachable");
    }

    #[test]
    fn experiment_focus_must_cover_successful_run_minimum() {
        let mut ruleset = valid_ruleset(ChallengeTemplateV1::Replication);
        let successful = ruleset.phase_gates[2]
            .requirements
            .iter_mut()
            .find(|requirement| requirement.kind == ChallengeRequirementKindV1::SuccessfulRuns)
            .expect("successful run requirement");
        successful.minimum = 2;
        let mut gameplay = typed_gameplay();
        gameplay.role_resources = Some(ChallengeRoleResourcesV1 {
            captain_focus: 1,
            evidence_focus: 1,
            experiment_focus: 1,
            run_budget: 2,
            retained_failure_focus_refund: 1,
        });
        ruleset.gameplay = Some(gameplay);

        let error = ruleset
            .validate()
            .expect_err("successful runs permanently spend experiment focus");
        assert!(error.contains("experiment_focus must be at least 2"));

        ruleset
            .gameplay
            .as_mut()
            .and_then(|gameplay| gameplay.role_resources.as_mut())
            .expect("role resources")
            .experiment_focus = 2;
        ruleset
            .validate()
            .expect("successful-run focus is reachable");
    }

    #[test]
    fn typed_gameplay_rejects_noncanonical_or_unsafe_content() {
        let mut gameplay = typed_gameplay();
        gameplay.objective = " surrounding whitespace ".into();
        assert!(gameplay.validate().unwrap_err().contains("whitespace"));

        gameplay = typed_gameplay();
        gameplay.risk = "line one\nline two".into();
        assert!(gameplay.validate().unwrap_err().contains("control"));

        gameplay = typed_gameplay();
        gameplay.modifiers = vec!["Frozen-Evaluator".into()];
        assert!(gameplay.validate().unwrap_err().contains("kebab-case"));

        gameplay = typed_gameplay();
        gameplay.modifiers = vec!["frozen-evaluator".into(), "frozen-evaluator".into()];
        assert!(gameplay.validate().unwrap_err().contains("duplicate"));

        gameplay = typed_gameplay();
        gameplay.modifiers = vec!["frozen-evaluator".into(), "failure-retention".into()];
        assert!(gameplay
            .validate()
            .unwrap_err()
            .contains("ascending lexical order"));

        gameplay = typed_gameplay();
        gameplay.victory_summary = "x".repeat(MAX_GAMEPLAY_TEXT_CHARS + 1);
        assert!(gameplay.validate().unwrap_err().contains("characters"));
    }

    #[test]
    fn mislabeled_template_and_duplicate_gate_fail_closed() {
        let mut benchmark = valid_ruleset(ChallengeTemplateV1::BenchmarkAblation);
        benchmark.phase_gates[2]
            .requirements
            .retain(|item| item.kind != ChallengeRequirementKindV1::RetainedFailedRuns);
        benchmark
            .victory_requirements
            .push(requirement(ChallengeRequirementKindV1::RetainedFailedRuns));
        assert!(benchmark
            .validate()
            .unwrap_err()
            .contains("retained_failed_runs"));

        let mut replication = valid_ruleset(ChallengeTemplateV1::Replication);
        replication.phase_gates[1].transition = replication.phase_gates[0].transition;
        assert!(replication.validate().unwrap_err().contains("duplicate"));
    }

    #[test]
    fn authoritative_snapshot_rejects_schema_and_ruleset_hash_drift() {
        let ruleset = valid_ruleset(ChallengeTemplateV1::Replication);
        let ruleset_hash = ruleset.canonical_hash().expect("ruleset hash");
        let mut snapshot = PaperChallengeRulesetSnapshotV1 {
            schema: CHALLENGE_RULESET_SNAPSHOT_V1.into(),
            challenge_snapshot_hash: format!("sha256:{}", "11".repeat(32)),
            ruleset_version: "replication-v1".into(),
            ruleset_hash,
            enforcement: ChallengeRulesetEnforcementV1::AuthoritativeV1,
            ruleset: Some(ruleset),
        };
        snapshot.validate().expect("valid snapshot");

        snapshot.schema = "hepta.paper_raid.challenge_ruleset_snapshot.v0".into();
        assert!(snapshot.validate().unwrap_err().contains("snapshot schema"));
        snapshot.schema = CHALLENGE_RULESET_SNAPSHOT_V1.into();
        snapshot.ruleset_hash = format!("sha256:{}", "22".repeat(32));
        assert!(snapshot.validate().unwrap_err().contains("does not match"));
    }

    #[test]
    fn legacy_snapshot_preserves_historical_uppercase_digest_compatibility() {
        let snapshot = PaperChallengeRulesetSnapshotV1 {
            schema: CHALLENGE_RULESET_SNAPSHOT_V1.into(),
            challenge_snapshot_hash: format!("sha256:{}", "33".repeat(32)),
            ruleset_version: "legacy-golden-v2".into(),
            ruleset_hash: format!("sha256:{}", "AB".repeat(32)),
            enforcement: ChallengeRulesetEnforcementV1::LegacyUnranked,
            ruleset: None,
        };
        snapshot.validate().expect("legacy uppercase digest");
        snapshot.canonical_hash().expect("legacy snapshot hash");
    }
}
