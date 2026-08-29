use super::*;

pub const ROLE_RESOURCE_STATE_V1: &str = "hepta.paper_raid.role_resources.v1";
pub const ROLE_RESOURCE_PROJECTION_V1: &str = "hepta.paper_raid.role_resource_projection.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalAuthorRoleV1 {
    Captain,
    Evidence,
    Experiment,
}

impl CanonicalAuthorRoleV1 {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "captain" => Some(Self::Captain),
            "evidence" => Some(Self::Evidence),
            "experiment" => Some(Self::Experiment),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleResourceActionKindV1 {
    EvidenceAssessment,
    ExperimentRun,
    CaptainCheckpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleResourceActionRecordV1 {
    pub action_id: Uuid,
    pub kind: RoleResourceActionKindV1,
    pub actor_player_id: Uuid,
    pub actor_role: CanonicalAuthorRoleV1,
    pub subject_id: Uuid,
    pub focus_spent: u16,
    pub focus_refunded: u16,
    pub run_budget_spent: u16,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleResourceStateV1 {
    pub schema: String,
    pub allocation: crate::ChallengeRoleResourcesV1,
    pub captain_focus_remaining: u16,
    pub evidence_focus_remaining: u16,
    pub experiment_focus_remaining: u16,
    pub run_budget_remaining: u16,
    pub actions: Vec<RoleResourceActionRecordV1>,
    pub version: u64,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligibility: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RoleResourceStateV1 {
    pub fn new(
        allocation: crate::ChallengeRoleResourcesV1,
        now: DateTime<Utc>,
    ) -> Result<Self, String> {
        allocation.validate()?;
        let state = Self {
            schema: ROLE_RESOURCE_STATE_V1.to_string(),
            captain_focus_remaining: allocation.captain_focus,
            evidence_focus_remaining: allocation.evidence_focus,
            experiment_focus_remaining: allocation.experiment_focus,
            run_budget_remaining: allocation.run_budget,
            allocation,
            actions: Vec::new(),
            version: 1,
            ranking_eligible: false,
            reward_eligible: false,
            economic_eligibility: false,
            created_at: now,
            updated_at: now,
        };
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != ROLE_RESOURCE_STATE_V1 {
            return Err(format!(
                "role resource schema must equal {ROLE_RESOURCE_STATE_V1}"
            ));
        }
        self.allocation.validate()?;
        if self.ranking_eligible || self.reward_eligible || self.economic_eligibility {
            return Err(
                "role resources are non-economic and cannot unlock ranking, reward, or economic eligibility"
                    .into(),
            );
        }
        let expected_version = u64::try_from(self.actions.len())
            .map_err(|_| "role resource action count overflow".to_string())?
            .checked_add(1)
            .ok_or_else(|| "role resource version overflow".to_string())?;
        if self.version != expected_version {
            return Err("role resource version must equal replayed action count plus one".into());
        }
        if self.updated_at < self.created_at {
            return Err("role resource updated_at cannot precede created_at".into());
        }

        let mut captain_focus = self.allocation.captain_focus;
        let mut evidence_focus = self.allocation.evidence_focus;
        let mut experiment_focus = self.allocation.experiment_focus;
        let mut run_budget = self.allocation.run_budget;
        let mut evidence_actions = 0usize;
        let mut experiment_actions = 0usize;
        let mut captain_actions = 0usize;
        let mut action_ids = HashSet::new();
        let mut evidence_subjects = HashSet::new();
        let mut run_subjects = HashSet::new();
        let mut previous_at = self.created_at;

        for action in &self.actions {
            if action.action_id.is_nil()
                || action.actor_player_id.is_nil()
                || action.subject_id.is_nil()
            {
                return Err("role resource action, actor, and subject IDs must be non-nil".into());
            }
            if !action_ids.insert(action.action_id) {
                return Err("role resource action IDs must be unique".into());
            }
            if action.occurred_at < previous_at || action.occurred_at > self.updated_at {
                return Err("role resource actions must use monotonic authoritative time".into());
            }
            previous_at = action.occurred_at;
            match action.kind {
                RoleResourceActionKindV1::EvidenceAssessment => {
                    if action.actor_role != CanonicalAuthorRoleV1::Evidence
                        || action.focus_spent != 1
                        || action.focus_refunded != 0
                        || action.run_budget_spent != 0
                    {
                        return Err(
                            "evidence assessment has an invalid resource delta or role".into()
                        );
                    }
                    if !evidence_subjects.insert(action.subject_id) {
                        return Err("an evidence card can only earn one assessment action".into());
                    }
                    evidence_focus = evidence_focus
                        .checked_sub(1)
                        .ok_or_else(|| "evidence focus overspent".to_string())?;
                    evidence_actions += 1;
                }
                RoleResourceActionKindV1::ExperimentRun => {
                    if action.actor_role != CanonicalAuthorRoleV1::Experiment
                        || action.focus_spent != 1
                        || action.run_budget_spent != 1
                        || action.focus_refunded > self.allocation.retained_failure_focus_refund
                    {
                        return Err("experiment run has an invalid resource delta or role".into());
                    }
                    if !run_subjects.insert(action.subject_id) {
                        return Err("a run can only consume role resources once".into());
                    }
                    experiment_focus = experiment_focus
                        .checked_sub(1)
                        .ok_or_else(|| "experiment focus overspent".to_string())?;
                    run_budget = run_budget
                        .checked_sub(1)
                        .ok_or_else(|| "run budget overspent".to_string())?;
                    experiment_focus = experiment_focus
                        .checked_add(action.focus_refunded)
                        .filter(|value| *value <= self.allocation.experiment_focus)
                        .ok_or_else(|| {
                            "failure-retention focus refund exceeds allocation".to_string()
                        })?;
                    experiment_actions += 1;
                }
                RoleResourceActionKindV1::CaptainCheckpoint => {
                    if action.actor_role != CanonicalAuthorRoleV1::Captain
                        || action.focus_spent != 1
                        || action.focus_refunded != 0
                        || action.run_budget_spent != 0
                        || action.subject_id != action.action_id
                    {
                        return Err(
                            "captain checkpoint has an invalid resource delta, subject, or role"
                                .into(),
                        );
                    }
                    if evidence_actions <= captain_actions || experiment_actions <= captain_actions
                    {
                        return Err(
                            "captain checkpoint requires a new evidence assessment and experiment run"
                                .into(),
                        );
                    }
                    captain_focus = captain_focus
                        .checked_sub(1)
                        .ok_or_else(|| "captain focus overspent".to_string())?;
                    captain_actions += 1;
                }
            }
        }

        if self.captain_focus_remaining != captain_focus
            || self.evidence_focus_remaining != evidence_focus
            || self.experiment_focus_remaining != experiment_focus
            || self.run_budget_remaining != run_budget
        {
            return Err("role resource balances do not match deterministic action replay".into());
        }
        if self
            .actions
            .last()
            .is_some_and(|last| last.occurred_at != self.updated_at)
        {
            return Err("role resource updated_at must equal the latest action time".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RoleResourceActionInputV1 {
    EvidenceAssessment { evidence_card_id: Uuid },
    CaptainCheckpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRoleResourceActionRequestV1 {
    pub action_id: Uuid,
    pub expected_resource_version: u64,
    pub action: RoleResourceActionInputV1,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleResourceActionResponseV1 {
    pub action: RoleResourceActionRecordV1,
    pub state: RoleResourceStateV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleResourceActionAvailabilityV1 {
    pub action: String,
    pub available: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleResourceProjectionV1 {
    pub schema: String,
    pub actor_role: String,
    pub state_version: u64,
    pub actor_focus_remaining: u16,
    pub run_budget_remaining: u16,
    pub evidence_assessment_count: usize,
    pub experiment_run_count: usize,
    pub captain_checkpoint_count: usize,
    pub retained_failure_count: usize,
    pub actions: Vec<RoleResourceActionAvailabilityV1>,
    pub ranking_eligible: bool,
    pub reward_eligible: bool,
    pub economic_eligibility: bool,
}

pub(crate) fn role_resource_state_from_snapshot(
    snapshot: &PaperChallengeRulesetSnapshotV1,
    now: DateTime<Utc>,
) -> Result<Option<RoleResourceStateV1>, ApiError> {
    let Some(allocation) = snapshot
        .ruleset
        .as_ref()
        .and_then(|ruleset| ruleset.gameplay.as_ref())
        .and_then(|gameplay| gameplay.role_resources.clone())
    else {
        return Ok(None);
    };
    RoleResourceStateV1::new(allocation, now)
        .map(Some)
        .map_err(|message| ApiError::internal(format!("initialize role resources: {message}")))
}

pub(crate) fn validate_paper_role_resources(paper: &PaperProject) -> Result<(), ApiError> {
    let expected = super::super::authoritative_paper_ruleset(paper)?
        .and_then(|ruleset| ruleset.gameplay.as_ref())
        .and_then(|gameplay| gameplay.role_resources.as_ref());
    match (expected, paper.role_resources.as_ref()) {
        (None, None) => Ok(()),
        (Some(expected), Some(state)) if &state.allocation == expected => {
            state.validate().map_err(|message| {
                ApiError::internal(format!("invalid role resource state: {message}"))
            })
        }
        _ => Err(ApiError::internal(
            "Paper role-resource authority disagrees with its frozen ChallengeRuleset snapshot",
        )),
    }
}

fn action_counts(state: &RoleResourceStateV1) -> (usize, usize, usize, usize) {
    let evidence = state
        .actions
        .iter()
        .filter(|action| action.kind == RoleResourceActionKindV1::EvidenceAssessment)
        .count();
    let experiment = state
        .actions
        .iter()
        .filter(|action| action.kind == RoleResourceActionKindV1::ExperimentRun)
        .count();
    let captain = state
        .actions
        .iter()
        .filter(|action| action.kind == RoleResourceActionKindV1::CaptainCheckpoint)
        .count();
    let retained_failure = state
        .actions
        .iter()
        .filter(|action| {
            action.kind == RoleResourceActionKindV1::ExperimentRun && action.focus_refunded > 0
        })
        .count();
    (evidence, experiment, captain, retained_failure)
}

pub(super) fn project_role_resources(
    paper: &PaperProject,
    actor_role: &str,
    evidence_card_count: usize,
    experiment_plan_count: usize,
    now: DateTime<Utc>,
) -> Option<RoleResourceProjectionV1> {
    let state = paper.role_resources.as_ref()?;
    if validate_paper_role_resources(paper).is_err() || state.validate().is_err() {
        return Some(RoleResourceProjectionV1 {
            schema: ROLE_RESOURCE_PROJECTION_V1.to_string(),
            actor_role: actor_role.to_string(),
            state_version: state.version,
            actor_focus_remaining: 0,
            run_budget_remaining: 0,
            evidence_assessment_count: 0,
            experiment_run_count: 0,
            captain_checkpoint_count: 0,
            retained_failure_count: 0,
            actions: [
                "assess_evidence",
                "create_run_record",
                "coordinate_checkpoint",
            ]
            .into_iter()
            .map(|action| RoleResourceActionAvailabilityV1 {
                action: action.into(),
                available: false,
                blockers: vec!["role_resource_state_invalid".into()],
            })
            .collect(),
            ranking_eligible: false,
            reward_eligible: false,
            economic_eligibility: false,
        });
    }
    let role = CanonicalAuthorRoleV1::parse(actor_role);
    let (evidence_count, experiment_count, captain_count, retained_failure_count) =
        action_counts(state);
    let actor_focus_remaining = match role {
        Some(CanonicalAuthorRoleV1::Captain) => state.captain_focus_remaining,
        Some(CanonicalAuthorRoleV1::Evidence) => state.evidence_focus_remaining,
        Some(CanonicalAuthorRoleV1::Experiment) => state.experiment_focus_remaining,
        None => 0,
    };

    let mut common_blockers = Vec::new();
    if paper.outcome != PaperChallengeOutcomeV1::InProgress {
        common_blockers.push(format!(
            "paper_challenge_terminal:{}",
            paper.outcome.as_str()
        ));
    } else if paper.active_rework_id.is_some()
        && paper
            .rework_expires_at
            .is_some_and(|expires_at| now >= expires_at)
    {
        common_blockers.push("rework_window_elapsed".to_string());
    } else if paper.active_rework_id.is_none()
        && paper
            .grace_expires_at
            .is_some_and(|grace_expires_at| now >= grace_expires_at)
    {
        common_blockers.push("paper_challenge_deadline_elapsed".to_string());
    }

    let mut evidence_blockers = common_blockers.clone();
    if role != Some(CanonicalAuthorRoleV1::Evidence) {
        evidence_blockers.push("evidence_role_required".to_string());
    }
    if state.evidence_focus_remaining == 0 {
        evidence_blockers.push("evidence_focus_exhausted".to_string());
    }
    if evidence_card_count <= evidence_count {
        evidence_blockers.push("unassessed_evidence_card_required".to_string());
    }

    let mut experiment_blockers = common_blockers.clone();
    if !CollaborationMutation::Run.allowed(paper.phase) {
        experiment_blockers.push(format!(
            "run_phase_disallows_action:{}",
            paper.phase.as_str()
        ));
    }
    if role != Some(CanonicalAuthorRoleV1::Experiment) {
        experiment_blockers.push("experiment_role_required".to_string());
    }
    if state.experiment_focus_remaining == 0 {
        experiment_blockers.push("experiment_focus_exhausted".to_string());
    }
    if state.run_budget_remaining == 0 {
        experiment_blockers.push("run_budget_exhausted".to_string());
    }
    if experiment_plan_count == 0 {
        experiment_blockers.push("experiment_plan_required".to_string());
    }

    let mut captain_blockers = common_blockers;
    if role != Some(CanonicalAuthorRoleV1::Captain) {
        captain_blockers.push("captain_role_required".to_string());
    }
    if state.captain_focus_remaining == 0 {
        captain_blockers.push("captain_focus_exhausted".to_string());
    }
    if evidence_count <= captain_count {
        captain_blockers.push("new_evidence_assessment_required".to_string());
    }
    if experiment_count <= captain_count {
        captain_blockers.push("new_experiment_run_required".to_string());
    }

    Some(RoleResourceProjectionV1 {
        schema: ROLE_RESOURCE_PROJECTION_V1.to_string(),
        actor_role: actor_role.to_string(),
        state_version: state.version,
        actor_focus_remaining,
        run_budget_remaining: state.run_budget_remaining,
        evidence_assessment_count: evidence_count,
        experiment_run_count: experiment_count,
        captain_checkpoint_count: captain_count,
        retained_failure_count,
        actions: vec![
            RoleResourceActionAvailabilityV1 {
                action: "assess_evidence".into(),
                available: evidence_blockers.is_empty(),
                blockers: evidence_blockers,
            },
            RoleResourceActionAvailabilityV1 {
                action: "create_run_record".into(),
                available: experiment_blockers.is_empty(),
                blockers: experiment_blockers,
            },
            RoleResourceActionAvailabilityV1 {
                action: "coordinate_checkpoint".into(),
                available: captain_blockers.is_empty(),
                blockers: captain_blockers,
            },
        ],
        ranking_eligible: false,
        reward_eligible: false,
        economic_eligibility: false,
    })
}

fn append_role_resource_action(
    state: &mut RoleResourceStateV1,
    action: RoleResourceActionRecordV1,
) -> Result<(), ApiError> {
    if state
        .actions
        .iter()
        .any(|existing| existing.action_id == action.action_id)
    {
        return Err(ApiError::conflict(
            "role_resource_action_exists",
            "role resource action_id already exists",
        ));
    }
    let mut next = state.clone();
    next.actions.push(action);
    next.version = next
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("role resource version overflow"))?;
    next.updated_at = next
        .actions
        .last()
        .expect("just appended role resource action")
        .occurred_at;
    next.validate()
        .map_err(|message| ApiError::internal(format!("role resource replay failed: {message}")))?;
    *state = next;
    Ok(())
}

pub(super) fn apply_run_role_resources(
    paper: &mut PaperProject,
    actor_player_id: Uuid,
    run: &RunRecord,
    now: DateTime<Utc>,
) -> Result<Option<RoleResourceActionRecordV1>, ApiError> {
    validate_paper_role_resources(paper)?;
    let Some(state) = paper.role_resources.as_ref() else {
        return Ok(None);
    };
    let next_paper_version = paper
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("paper version overflow"))?;
    state
        .validate()
        .map_err(|message| ApiError::internal(format!("invalid role resource state: {message}")))?;
    let mut next_state = state.clone();
    let action_at = now.max(state.updated_at).max(paper.updated_at);
    if next_state.experiment_focus_remaining == 0 {
        return Err(ApiError::conflict(
            "experiment_focus_exhausted",
            "the Experiment role has no focus remaining",
        ));
    }
    if next_state.run_budget_remaining == 0 {
        return Err(ApiError::conflict(
            "run_budget_exhausted",
            "the challenge has no run budget remaining",
        ));
    }
    if next_state.actions.iter().any(|action| {
        action.kind == RoleResourceActionKindV1::ExperimentRun
            && action.subject_id == run.run_record_id
    }) {
        return Err(ApiError::conflict(
            "run_resources_already_consumed",
            "run record already consumed role resources",
        ));
    }

    next_state.experiment_focus_remaining -= 1;
    next_state.run_budget_remaining -= 1;
    let retained_failure = run.status == RunStatus::Failed && run.failure_hash.is_some();
    let focus_refunded = if retained_failure {
        next_state
            .allocation
            .retained_failure_focus_refund
            .min(next_state.allocation.experiment_focus - next_state.experiment_focus_remaining)
    } else {
        0
    };
    next_state.experiment_focus_remaining += focus_refunded;
    let action = RoleResourceActionRecordV1 {
        action_id: run.run_record_id,
        kind: RoleResourceActionKindV1::ExperimentRun,
        actor_player_id,
        actor_role: CanonicalAuthorRoleV1::Experiment,
        subject_id: run.run_record_id,
        focus_spent: 1,
        focus_refunded,
        run_budget_spent: 1,
        occurred_at: action_at,
    };
    append_role_resource_action(&mut next_state, action.clone())?;
    paper.role_resources = Some(next_state);
    paper.version = next_paper_version;
    paper.updated_at = action_at;
    Ok(Some(action))
}

fn validate_role_resource_action_request(
    request: &CreateRoleResourceActionRequestV1,
) -> Result<(), ApiError> {
    validate_idempotency_key(&request.idempotency_key)?;
    if request.action_id.is_nil() {
        return Err(ApiError::bad_request(
            "invalid_role_resource_action_id",
            "action_id must be non-nil",
        ));
    }
    if request.expected_resource_version == 0
        || request.expected_resource_version > JSON_SAFE_U64_MAX
    {
        return Err(ApiError::bad_request(
            "invalid_role_resource_version",
            "expected_resource_version must be a positive JSON-safe integer",
        ));
    }
    if matches!(
        &request.action,
        RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id } if evidence_card_id.is_nil()
    ) {
        return Err(ApiError::bad_request(
            "invalid_evidence_card_id",
            "evidence_card_id must be non-nil",
        ));
    }
    Ok(())
}

fn apply_explicit_role_resource_action(
    paper: &mut PaperProject,
    team: &ResearchTeam,
    actor_player_id: Uuid,
    request: &CreateRoleResourceActionRequestV1,
    evidence_exists: bool,
    now: DateTime<Utc>,
) -> Result<RoleResourceActionRecordV1, ApiError> {
    ensure_paper_gameplay_active(paper, now)?;
    validate_paper_role_resources(paper)?;
    super::super::require_canonical_author_role_contract(team)?;
    let state = paper.role_resources.as_ref().ok_or_else(|| {
        ApiError::conflict(
            "role_resources_not_enabled",
            "the frozen ChallengeRuleset does not enable role resources",
        )
    })?;
    let next_paper_version = paper
        .version
        .checked_add(1)
        .ok_or_else(|| ApiError::internal("paper version overflow"))?;
    state
        .validate()
        .map_err(|message| ApiError::internal(format!("invalid role resource state: {message}")))?;
    let mut next_state = state.clone();
    let action_at = now.max(state.updated_at).max(paper.updated_at);
    if next_state.version != request.expected_resource_version {
        return Err(version_conflict(
            "role resource state",
            request.expected_resource_version,
            next_state.version,
        ));
    }
    let actor_role = team
        .members
        .iter()
        .find(|member| member.player_id == actor_player_id)
        .and_then(|member| CanonicalAuthorRoleV1::parse(&member.role))
        .ok_or_else(|| {
            ApiError::forbidden(
                "canonical_author_role_required",
                "role resource actions require captain, evidence, or experiment role",
            )
        })?;
    let (evidence_count, experiment_count, captain_count, _) = action_counts(&next_state);
    let action = match &request.action {
        RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id } => {
            if actor_role != CanonicalAuthorRoleV1::Evidence {
                return Err(ApiError::forbidden(
                    "evidence_role_required",
                    "only the Evidence role can spend focus on evidence assessment",
                ));
            }
            if !evidence_exists {
                return Err(ApiError::bad_request(
                    "evidence_card_not_found",
                    "evidence assessment must reference an evidence card in this Paper",
                ));
            }
            if next_state.actions.iter().any(|action| {
                action.kind == RoleResourceActionKindV1::EvidenceAssessment
                    && action.subject_id == *evidence_card_id
            }) {
                return Err(ApiError::conflict(
                    "evidence_already_assessed",
                    "an evidence card can only earn one assessment action",
                ));
            }
            next_state.evidence_focus_remaining = next_state
                .evidence_focus_remaining
                .checked_sub(1)
                .ok_or_else(|| {
                    ApiError::conflict(
                        "evidence_focus_exhausted",
                        "the Evidence role has no focus remaining",
                    )
                })?;
            RoleResourceActionRecordV1 {
                action_id: request.action_id,
                kind: RoleResourceActionKindV1::EvidenceAssessment,
                actor_player_id,
                actor_role,
                subject_id: *evidence_card_id,
                focus_spent: 1,
                focus_refunded: 0,
                run_budget_spent: 0,
                occurred_at: action_at,
            }
        }
        RoleResourceActionInputV1::CaptainCheckpoint => {
            if actor_role != CanonicalAuthorRoleV1::Captain {
                return Err(ApiError::forbidden(
                    "captain_role_required",
                    "only the Captain role can spend focus on a team checkpoint",
                ));
            }
            if evidence_count <= captain_count || experiment_count <= captain_count {
                return Err(ApiError::conflict(
                    "checkpoint_dependencies_incomplete",
                    "a checkpoint requires one new evidence assessment and one new experiment run",
                ));
            }
            next_state.captain_focus_remaining = next_state
                .captain_focus_remaining
                .checked_sub(1)
                .ok_or_else(|| {
                    ApiError::conflict(
                        "captain_focus_exhausted",
                        "the Captain role has no focus remaining",
                    )
                })?;
            RoleResourceActionRecordV1 {
                action_id: request.action_id,
                kind: RoleResourceActionKindV1::CaptainCheckpoint,
                actor_player_id,
                actor_role,
                subject_id: request.action_id,
                focus_spent: 1,
                focus_refunded: 0,
                run_budget_spent: 0,
                occurred_at: action_at,
            }
        }
    };
    append_role_resource_action(&mut next_state, action.clone())?;
    paper.role_resources = Some(next_state);
    paper.version = next_paper_version;
    paper.updated_at = action_at;
    Ok(action)
}

pub(super) async fn persist_role_resource_paper_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper: &PaperProject,
    previous_paper_version: u64,
) -> Result<(), ApiError> {
    let record_json = serde_json::to_value(paper)
        .map_err(|error| ApiError::internal(format!("encode role-resource Paper: {error}")))?;
    let updated = sqlx::query(
        "update hepta_paper_projects
         set version=$1,record_json=$2::jsonb,updated_at=$3
         where paper_project_id=$4 and version=$5",
    )
    .bind(i64::try_from(paper.version).map_err(|_| ApiError::internal("paper version overflow"))?)
    .bind(record_json)
    .bind(paper.updated_at)
    .bind(paper.paper_project_id)
    .bind(
        i64::try_from(previous_paper_version)
            .map_err(|_| ApiError::internal("paper version overflow"))?,
    )
    .execute(&mut **tx)
    .await
    .map_err(ApiError::database)?;
    if updated.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "role_resource_concurrent_update",
            "Paper role resources changed concurrently",
        ));
    }
    Ok(())
}

pub(super) async fn paper_and_team_for_role_resource_mutation_postgres(
    tx: &mut Transaction<'_, Postgres>,
    paper_id: Uuid,
    assertion: &ConsumerUserAssertionClaimV2,
) -> Result<(PaperProject, ResearchTeam), ApiError> {
    let paper_row = sqlx::query(
        "select version,record_json from hepta_paper_projects
         where paper_project_id=$1 for update",
    )
    .bind(paper_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(ApiError::database)?
    .ok_or_else(|| {
        ApiError::not_found("paper_project_not_found", "paper project does not exist")
    })?;
    let paper: PaperProject = decode_record(paper_row.get("record_json"), "paper project")?;
    let relational_version = u64::try_from(paper_row.get::<i64, _>("version"))
        .map_err(|_| ApiError::internal("negative paper version"))?;
    if paper.version != relational_version {
        return Err(ApiError::internal(
            "Paper role-resource authority disagrees with relational version",
        ));
    }
    assert_team_actor_postgres(tx, paper.team_id, assertion).await?;
    let team_row =
        sqlx::query("select record_json from hepta_research_teams where team_id=$1 for share")
            .bind(paper.team_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(ApiError::database)?;
    let team = decode_record(team_row.get("record_json"), "research team")?;
    Ok((paper, team))
}

async fn create_role_resource_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<CreateRoleResourceActionRequestV1>,
) -> Result<(StatusCode, Json<RoleResourceActionResponseV1>), ApiError> {
    const OPERATION: &str = "create_role_resource_action_v1";
    validate_role_resource_action_request(&request)?;
    let path = format!("/v2/hepta/papers/{paper_id}/role-resources/actions");
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
        if let Some(replay) =
            memory_replay(&memory, OPERATION, &request.idempotency_key, &request_hash)?
        {
            return Ok(replay);
        }
        let (mut paper, team) = paper_and_team_memory(&memory, paper_id, &assertion)?;
        let now = Utc::now();
        let evidence_exists = match &request.action {
            RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id } => memory
                .collaboration
                .evidence_cards
                .get(evidence_card_id)
                .is_some_and(|card| card.paper_project_id == paper_id),
            RoleResourceActionInputV1::CaptainCheckpoint => false,
        };
        let action = apply_explicit_role_resource_action(
            &mut paper,
            &team,
            assertion.player_id,
            &request,
            evidence_exists,
            now,
        )?;
        let resource_state = paper
            .role_resources
            .clone()
            .ok_or_else(|| ApiError::internal("role resource state disappeared"))?;
        memory.papers.insert(paper_id, paper.clone());
        push_room_event_memory(
            &mut memory,
            OPERATION,
            &request.idempotency_key,
            "hepta.paper_raid.role_resource.action.v1",
            paper_id,
            request.action_id,
            1,
            json!({
                "action_id": action.action_id,
                "kind": action.kind,
                "actor_role": action.actor_role,
                "subject_id": action.subject_id,
                "resource_state_version": resource_state.version,
                "focus_spent": action.focus_spent,
                "focus_refunded": action.focus_refunded,
                "run_budget_spent": action.run_budget_spent,
                "ranking_eligible": false,
                "reward_eligible": false,
                "economic_eligibility": false,
            }),
        );
        let response = RoleResourceActionResponseV1 {
            action,
            state: resource_state,
        };
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
    let (mut paper, team) =
        paper_and_team_for_role_resource_mutation_postgres(&mut tx, paper_id, &assertion).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("select clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::database)?;
    let evidence_exists = match &request.action {
        RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id } => sqlx::query(
            "select 1 from hepta_evidence_cards
                 where evidence_card_id=$1 and paper_project_id=$2 for share",
        )
        .bind(*evidence_card_id)
        .bind(paper_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::database)?
        .is_some(),
        RoleResourceActionInputV1::CaptainCheckpoint => false,
    };
    let previous_paper_version = paper.version;
    let action = apply_explicit_role_resource_action(
        &mut paper,
        &team,
        assertion.player_id,
        &request,
        evidence_exists,
        now,
    )?;
    persist_role_resource_paper_postgres(&mut tx, &paper, previous_paper_version).await?;
    let resource_state = paper
        .role_resources
        .clone()
        .ok_or_else(|| ApiError::internal("role resource state disappeared"))?;
    insert_room_event_postgres(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        "hepta.paper_raid.role_resource.action.v1",
        paper_id,
        request.action_id,
        1,
        json!({
            "action_id": action.action_id,
            "kind": action.kind,
            "actor_role": action.actor_role,
            "subject_id": action.subject_id,
            "resource_state_version": resource_state.version,
            "focus_spent": action.focus_spent,
            "focus_refunded": action.focus_refunded,
            "run_budget_spent": action.run_budget_spent,
            "ranking_eligible": false,
            "reward_eligible": false,
            "economic_eligibility": false,
        }),
    )
    .await?;
    let response = RoleResourceActionResponseV1 {
        action,
        state: resource_state,
    };
    finish_postgres_idempotent(
        &mut tx,
        OPERATION,
        &request.idempotency_key,
        &request_hash,
        Some(request.action_id),
        StatusCode::CREATED,
        &response,
    )
    .await?;
    tx.commit().await.map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route(
        "/v2/hepta/papers/:paper_id/role-resources/actions",
        post(create_role_resource_action),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChallengeDifficultyV1, ChallengeGameplayV1, ChallengePhaseGateV1, ChallengeRulesetV1,
        ChallengeTemplateV1, CHALLENGE_RULESET_V1,
    };

    fn allocation() -> crate::ChallengeRoleResourcesV1 {
        crate::ChallengeRoleResourcesV1 {
            captain_focus: 2,
            evidence_focus: 2,
            experiment_focus: 2,
            run_budget: 2,
            retained_failure_focus_refund: 1,
        }
    }

    fn action(
        id: Uuid,
        kind: RoleResourceActionKindV1,
        role: CanonicalAuthorRoleV1,
        subject_id: Uuid,
        focus_refunded: u16,
        at: DateTime<Utc>,
    ) -> RoleResourceActionRecordV1 {
        RoleResourceActionRecordV1 {
            action_id: id,
            kind,
            actor_player_id: Uuid::new_v4(),
            actor_role: role,
            subject_id,
            focus_spent: 1,
            focus_refunded,
            run_budget_spent: if kind == RoleResourceActionKindV1::ExperimentRun {
                1
            } else {
                0
            },
            occurred_at: at,
        }
    }

    fn minimum(kind: ChallengeRequirementKindV1) -> ChallengeMinimumV1 {
        ChallengeMinimumV1 { kind, minimum: 1 }
    }

    fn ruleset_with_role_resources(
        role_resources: crate::ChallengeRoleResourcesV1,
    ) -> ChallengeRulesetV1 {
        use ChallengeForwardTransitionV1 as Transition;
        use ChallengeRequirementKindV1 as Requirement;

        ChallengeRulesetV1 {
            schema: CHALLENGE_RULESET_V1.into(),
            template: ChallengeTemplateV1::BenchmarkAblation,
            duration_seconds: 5_400,
            grace_seconds: 900,
            phase_gates: vec![
                ChallengePhaseGateV1 {
                    transition: Transition::PreregisteringToResearching,
                    requirements: vec![
                        minimum(Requirement::WorkItems),
                        minimum(Requirement::ArtifactManifests),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ResearchingToExperimenting,
                    requirements: vec![
                        minimum(Requirement::EvidenceCards),
                        minimum(Requirement::Citations),
                        minimum(Requirement::Claims),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ExperimentingToDrafting,
                    requirements: vec![
                        minimum(Requirement::ArtifactManifests),
                        minimum(Requirement::ExperimentPlans),
                        minimum(Requirement::SuccessfulRuns),
                        minimum(Requirement::RetainedFailedRuns),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::DraftingToIntegrityReview,
                    requirements: vec![
                        minimum(Requirement::AllWorkItemsTerminal),
                        minimum(Requirement::SectionRevisions),
                        minimum(Requirement::PaperRevisions),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::IntegrityReviewToReproducing,
                    requirements: vec![
                        minimum(Requirement::ApprovingSectionReviews),
                        minimum(Requirement::SectionMerges),
                    ],
                },
                ChallengePhaseGateV1 {
                    transition: Transition::ReproducingToAuthorApproval,
                    requirements: vec![minimum(Requirement::PaperRevisionCoversSectionMerges)],
                },
            ],
            victory_requirements: vec![
                minimum(Requirement::AcceptedWorkItems),
                minimum(Requirement::ReleaseCandidate),
                minimum(Requirement::AllAuthorConsents),
                minimum(Requirement::PaperRevisionCoversSectionMerges),
            ],
            gameplay: Some(ChallengeGameplayV1 {
                difficulty: ChallengeDifficultyV1::Intermediate,
                objective: "Test role resources.".into(),
                risk: "Test-only snapshot.".into(),
                modifiers: Vec::new(),
                victory_summary: "Test deterministic resource replay.".into(),
                role_resources: Some(role_resources),
            }),
        }
    }

    fn paper_with_role_resources(now: DateTime<Utc>) -> PaperProject {
        let frozen_allocation = allocation();
        let ruleset = ruleset_with_role_resources(frozen_allocation.clone());
        let ruleset_hash = ruleset.canonical_hash().expect("canonical ruleset");
        let snapshot = PaperChallengeRulesetSnapshotV1 {
            schema: CHALLENGE_RULESET_SNAPSHOT_V1.into(),
            challenge_snapshot_hash:
                "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into(),
            ruleset_version: "test-v1".into(),
            ruleset_hash,
            enforcement: ChallengeRulesetEnforcementV1::AuthoritativeV1,
            ruleset: Some(ruleset),
            material_authority: None,
        };
        let snapshot_hash = snapshot.canonical_hash().expect("canonical snapshot");
        PaperProject {
            paper_project_id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            challenge_id: Uuid::new_v4(),
            title: "Role resource test".into(),
            target_format: "paper".into(),
            phase: PaperPhase::Experimenting,
            challenge_ruleset_snapshot: Some(snapshot),
            challenge_ruleset_snapshot_hash: Some(snapshot_hash),
            deadline_at: None,
            grace_expires_at: None,
            active_rework_id: None,
            active_rework_cycle: None,
            rework_expires_at: None,
            outcome: PaperChallengeOutcomeV1::InProgress,
            outcome_reason: None,
            terminal_at: None,
            role_resources: Some(RoleResourceStateV1::new(frozen_allocation, now).expect("state")),
            current_revision_id: None,
            release_candidate_revision_id: None,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    fn run_record(
        paper_id: Uuid,
        status: RunStatus,
        failure_hash: Option<&str>,
        now: DateTime<Utc>,
    ) -> RunRecord {
        let succeeded = status == RunStatus::Succeeded;
        RunRecord {
            run_record_id: Uuid::new_v4(),
            paper_project_id: paper_id,
            experiment_plan_id: Uuid::new_v4(),
            status,
            seed: 7,
            parameters_hash:
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            logs_manifest_id: Uuid::new_v4(),
            outputs_manifest_id: succeeded.then(Uuid::new_v4),
            metrics_hash: succeeded.then(|| {
                "sha256:1212121212121212121212121212121212121212121212121212121212121212".into()
            }),
            failure_hash: failure_hash.map(str::to_string),
            version: 1,
            created_at: now,
        }
    }

    fn team_member(player_id: Uuid, role: &str, now: DateTime<Utc>) -> TeamMember {
        TeamMember {
            participant_slot: match role {
                "captain" => 1,
                "evidence" => 2,
                "experiment" => 3,
                _ => 9,
            },
            player_id,
            binding_id: Uuid::new_v4(),
            agent_id: format!("agent-{role}"),
            role: role.into(),
            joined_at: now,
        }
    }

    fn team_with_canonical_roles(
        paper: &PaperProject,
        now: DateTime<Utc>,
    ) -> (ResearchTeam, Uuid, Uuid, Uuid) {
        let captain_id = Uuid::new_v4();
        let evidence_id = Uuid::new_v4();
        let experiment_id = Uuid::new_v4();
        (
            ResearchTeam {
                team_id: paper.team_id,
                challenge_id: paper.challenge_id,
                collaboration_compact_hash:
                    "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into(),
                status: TeamStatus::Locked,
                roster_version: 1,
                members: vec![
                    team_member(captain_id, "captain", now),
                    team_member(evidence_id, "evidence", now),
                    team_member(experiment_id, "experiment", now),
                ],
                version: 1,
                created_at: now,
                updated_at: now,
            },
            captain_id,
            evidence_id,
            experiment_id,
        )
    }

    #[test]
    fn role_resource_replay_enforces_dependency_and_balances() {
        let now = Utc::now();
        let mut state = RoleResourceStateV1::new(allocation(), now).expect("state");
        let evidence_id = Uuid::new_v4();
        state.evidence_focus_remaining -= 1;
        append_role_resource_action(
            &mut state,
            action(
                Uuid::new_v4(),
                RoleResourceActionKindV1::EvidenceAssessment,
                CanonicalAuthorRoleV1::Evidence,
                evidence_id,
                0,
                now,
            ),
        )
        .expect("evidence action");
        state.experiment_focus_remaining -= 1;
        state.run_budget_remaining -= 1;
        let run_id = Uuid::new_v4();
        append_role_resource_action(
            &mut state,
            action(
                run_id,
                RoleResourceActionKindV1::ExperimentRun,
                CanonicalAuthorRoleV1::Experiment,
                run_id,
                0,
                now,
            ),
        )
        .expect("experiment action");
        state.captain_focus_remaining -= 1;
        let checkpoint_id = Uuid::new_v4();
        append_role_resource_action(
            &mut state,
            action(
                checkpoint_id,
                RoleResourceActionKindV1::CaptainCheckpoint,
                CanonicalAuthorRoleV1::Captain,
                checkpoint_id,
                0,
                now,
            ),
        )
        .expect("captain checkpoint");
        state.validate().expect("deterministic replay");
    }

    #[test]
    fn captain_cannot_checkpoint_before_both_other_roles_act() {
        let now = Utc::now();
        let mut state = RoleResourceStateV1::new(allocation(), now).expect("state");
        state.captain_focus_remaining -= 1;
        let checkpoint_id = Uuid::new_v4();
        let error = append_role_resource_action(
            &mut state,
            action(
                checkpoint_id,
                RoleResourceActionKindV1::CaptainCheckpoint,
                CanonicalAuthorRoleV1::Captain,
                checkpoint_id,
                0,
                now,
            ),
        )
        .expect_err("dependency must fail closed");
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn retained_failure_refund_is_non_economic_and_bounded() {
        let now = Utc::now();
        let mut state = RoleResourceStateV1::new(allocation(), now).expect("state");
        state.experiment_focus_remaining -= 1;
        state.run_budget_remaining -= 1;
        state.experiment_focus_remaining += 1;
        let run_id = Uuid::new_v4();
        append_role_resource_action(
            &mut state,
            action(
                run_id,
                RoleResourceActionKindV1::ExperimentRun,
                CanonicalAuthorRoleV1::Experiment,
                run_id,
                1,
                now,
            ),
        )
        .expect("failure disclosure");
        assert_eq!(
            state.experiment_focus_remaining,
            allocation().experiment_focus
        );
        assert_eq!(state.run_budget_remaining, allocation().run_budget - 1);
        assert!(!state.ranking_eligible);
        assert!(!state.reward_eligible);
        assert!(!state.economic_eligibility);
    }

    #[test]
    fn failed_run_with_retained_failure_refunds_focus_but_still_spends_run_budget() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let run = run_record(
            paper.paper_project_id,
            RunStatus::Failed,
            Some("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            now,
        );

        let action = apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, now)
            .expect("retained failed run")
            .expect("resource action");
        let state = paper.role_resources.expect("role resources");

        assert_eq!(action.focus_refunded, 1);
        assert_eq!(
            state.experiment_focus_remaining,
            allocation().experiment_focus
        );
        assert_eq!(state.run_budget_remaining, allocation().run_budget - 1);
        assert_eq!(state.version, 2);
        state.validate().expect("replayable state");
    }

    #[test]
    fn cancelled_run_never_receives_retained_failure_refund() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let run = run_record(
            paper.paper_project_id,
            RunStatus::Cancelled,
            Some("sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"),
            now,
        );

        let action = apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, now)
            .expect("cancelled run")
            .expect("resource action");
        let state = paper.role_resources.expect("role resources");

        assert_eq!(action.focus_refunded, 0);
        assert_eq!(
            state.experiment_focus_remaining,
            allocation().experiment_focus - 1
        );
        assert_eq!(state.run_budget_remaining, allocation().run_budget - 1);
        state.validate().expect("replayable state");
    }

    #[test]
    fn same_run_cannot_consume_resources_twice() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let run = run_record(paper.paper_project_id, RunStatus::Succeeded, None, now);

        apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, now)
            .expect("first application")
            .expect("resource action");
        let after_first = paper.clone();
        let error = apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, now)
            .expect_err("duplicate run must fail closed");

        assert_eq!(error.code, "run_resources_already_consumed");
        assert_eq!(paper, after_first, "duplicate must not mutate balances");
    }

    #[test]
    fn run_budget_overspend_fails_without_partial_mutation() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        for _ in 0..allocation().run_budget {
            let run = run_record(
                paper.paper_project_id,
                RunStatus::Failed,
                Some("sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"),
                now,
            );
            apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, now)
                .expect("budgeted run")
                .expect("resource action");
        }
        let exhausted = paper.clone();
        let extra = run_record(paper.paper_project_id, RunStatus::Failed, None, now);
        let error = apply_run_role_resources(&mut paper, Uuid::new_v4(), &extra, now)
            .expect_err("overspend must fail closed");

        assert_eq!(error.code, "run_budget_exhausted");
        assert_eq!(
            paper, exhausted,
            "overspend must not partially mutate Paper"
        );
    }

    #[test]
    fn role_mismatch_is_rejected_without_spending_focus() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let (team, captain_id, _, _) = team_with_canonical_roles(&paper, now);
        let before = paper.clone();
        let request = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 1,
            action: RoleResourceActionInputV1::EvidenceAssessment {
                evidence_card_id: Uuid::new_v4(),
            },
            idempotency_key: "role-mismatch".into(),
        };

        let error =
            apply_explicit_role_resource_action(&mut paper, &team, captain_id, &request, true, now)
                .expect_err("Captain must not spend Evidence focus");

        assert_eq!(error.code, "evidence_role_required");
        assert_eq!(paper, before, "forbidden action must be side-effect free");
    }

    #[test]
    fn evidence_card_can_consume_evidence_focus_exactly_once() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let (team, _, evidence_id, _) = team_with_canonical_roles(&paper, now);
        let evidence_card_id = Uuid::new_v4();
        let request = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 1,
            action: RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id },
            idempotency_key: "evidence-once".into(),
        };

        apply_explicit_role_resource_action(&mut paper, &team, evidence_id, &request, true, now)
            .expect("first assessment");
        let after_first = paper.clone();
        let duplicate = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 2,
            action: RoleResourceActionInputV1::EvidenceAssessment { evidence_card_id },
            idempotency_key: "evidence-duplicate".into(),
        };
        let error = apply_explicit_role_resource_action(
            &mut paper,
            &team,
            evidence_id,
            &duplicate,
            true,
            now,
        )
        .expect_err("duplicate evidence assessment must fail closed");

        assert_eq!(error.code, "evidence_already_assessed");
        assert_eq!(paper, after_first, "duplicate must not spend focus twice");
        assert_eq!(
            paper
                .role_resources
                .as_ref()
                .expect("role resources")
                .evidence_focus_remaining,
            allocation().evidence_focus - 1
        );
    }

    #[test]
    fn captain_checkpoint_requires_fresh_actions_from_both_other_roles() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let (team, captain_id, evidence_id, experiment_id) = team_with_canonical_roles(&paper, now);
        let evidence = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 1,
            action: RoleResourceActionInputV1::EvidenceAssessment {
                evidence_card_id: Uuid::new_v4(),
            },
            idempotency_key: "checkpoint-evidence".into(),
        };
        apply_explicit_role_resource_action(&mut paper, &team, evidence_id, &evidence, true, now)
            .expect("evidence dependency");
        let run = run_record(paper.paper_project_id, RunStatus::Succeeded, None, now);
        apply_run_role_resources(&mut paper, experiment_id, &run, now)
            .expect("experiment dependency")
            .expect("resource action");
        let checkpoint = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 3,
            action: RoleResourceActionInputV1::CaptainCheckpoint,
            idempotency_key: "checkpoint-first".into(),
        };
        apply_explicit_role_resource_action(&mut paper, &team, captain_id, &checkpoint, false, now)
            .expect("checkpoint after both dependencies");

        let after_first = paper.clone();
        let repeated = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 4,
            action: RoleResourceActionInputV1::CaptainCheckpoint,
            idempotency_key: "checkpoint-repeat".into(),
        };
        let error = apply_explicit_role_resource_action(
            &mut paper, &team, captain_id, &repeated, false, now,
        )
        .expect_err("checkpoint needs fresh dependencies");

        assert_eq!(error.code, "checkpoint_dependencies_incomplete");
        assert_eq!(paper, after_first, "failed checkpoint must not spend focus");
    }

    #[test]
    fn invalid_authority_projects_only_blocked_non_economic_actions() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        paper
            .role_resources
            .as_mut()
            .expect("role resources")
            .reward_eligible = true;

        let projection = project_role_resources(&paper, "experiment", 1, 1, now)
            .expect("invalid state remains visibly fail-closed");

        assert_eq!(projection.actor_focus_remaining, 0);
        assert_eq!(projection.run_budget_remaining, 0);
        assert!(projection.actions.iter().all(|action| {
            !action.available && action.blockers == vec!["role_resource_state_invalid".to_string()]
        }));
        assert!(!projection.ranking_eligible);
        assert!(!projection.reward_eligible);
        assert!(!projection.economic_eligibility);
    }

    #[test]
    fn resource_allocation_must_match_the_frozen_challenge_snapshot() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        paper
            .role_resources
            .as_mut()
            .expect("role resources")
            .allocation
            .run_budget += 1;

        let error = validate_paper_role_resources(&paper)
            .expect_err("allocation drift must fail closed before mutation");

        assert_eq!(error.code, "internal_error");
        assert!(error.message.contains("frozen ChallengeRuleset"));
    }

    #[test]
    fn matching_state_and_mutated_ruleset_still_reject_stale_snapshot_hash() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let snapshot = paper
            .challenge_ruleset_snapshot
            .as_mut()
            .expect("challenge snapshot");
        let ruleset = snapshot.ruleset.as_mut().expect("typed ruleset");
        let snapshot_allocation = ruleset
            .gameplay
            .as_mut()
            .and_then(|gameplay| gameplay.role_resources.as_mut())
            .expect("snapshot allocation");
        snapshot_allocation.run_budget = 3;
        snapshot.ruleset_hash = ruleset.canonical_hash().expect("mutated ruleset hash");
        let state = paper.role_resources.as_mut().expect("role resources");
        state.allocation.run_budget = 3;
        state.run_budget_remaining = 3;
        state.validate().expect("mutated state remains replayable");

        let error = validate_paper_role_resources(&paper)
            .expect_err("the immutable snapshot hash must bind snapshot and state together");

        assert_eq!(error.code, "internal_error");
        assert!(error.message.contains("snapshot hash"));
    }

    #[test]
    fn explicit_actions_require_the_canonical_roster_contract() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        let (mut team, _, evidence_id, _) = team_with_canonical_roles(&paper, now);
        team.members[2].role = "evidence".into();
        let before = paper.clone();
        let request = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 1,
            action: RoleResourceActionInputV1::EvidenceAssessment {
                evidence_card_id: Uuid::new_v4(),
            },
            idempotency_key: "invalid-roster".into(),
        };

        let error = apply_explicit_role_resource_action(
            &mut paper,
            &team,
            evidence_id,
            &request,
            true,
            now,
        )
        .expect_err("duplicate Evidence and missing Experiment must fail closed");

        assert_eq!(error.code, "team_role_contract_required");
        assert_eq!(paper, before, "invalid roster must not spend resources");
    }

    #[test]
    fn terminal_and_elapsed_challenges_project_no_available_role_actions() {
        let now = Utc::now();
        let mut terminal = paper_with_role_resources(now);
        terminal.outcome = PaperChallengeOutcomeV1::Expired;
        let projection = project_role_resources(&terminal, "experiment", 1, 1, now)
            .expect("terminal projection");
        assert!(projection.actions.iter().all(|action| {
            !action.available
                && action
                    .blockers
                    .contains(&"paper_challenge_terminal:expired".to_string())
        }));

        let mut elapsed = paper_with_role_resources(now);
        elapsed.grace_expires_at = Some(now);
        let projection =
            project_role_resources(&elapsed, "experiment", 1, 1, now).expect("elapsed projection");
        assert!(projection.actions.iter().all(|action| {
            !action.available
                && action
                    .blockers
                    .contains(&"paper_challenge_deadline_elapsed".to_string())
        }));
    }

    #[test]
    fn run_availability_fails_closed_outside_a_run_phase() {
        let now = Utc::now();
        let mut paper = paper_with_role_resources(now);
        paper.phase = PaperPhase::Researching;

        let projection =
            project_role_resources(&paper, "experiment", 1, 1, now).expect("role projection");
        let run = projection
            .actions
            .iter()
            .find(|action| action.action == "create_run_record")
            .expect("run action");

        assert!(!run.available);
        assert!(run
            .blockers
            .contains(&"run_phase_disallows_action:researching".to_string()));
    }

    #[test]
    fn run_handler_guard_uses_the_supplied_authoritative_deadline_clock() {
        let boundary = Utc::now();
        let mut paper = paper_with_role_resources(boundary);
        paper.grace_expires_at = Some(boundary);

        require_collaboration_phase_at(
            &paper,
            CollaborationMutation::Run,
            boundary - chrono::Duration::nanoseconds(1),
        )
        .expect("the instant before the grace boundary remains active");
        let error = require_collaboration_phase_at(&paper, CollaborationMutation::Run, boundary)
            .expect_err("the database-authoritative boundary is fail-closed");
        assert_eq!(error.code, "paper_challenge_deadline_elapsed");

        paper.grace_expires_at = None;
        paper.phase = PaperPhase::Researching;
        let error = require_collaboration_phase_at(&paper, CollaborationMutation::Run, boundary)
            .expect_err("RunRecord creation is phase-gated under the same supplied clock");
        assert_eq!(error.code, "paper_phase_disallows_collaboration_mutation");
    }

    #[test]
    fn explicit_action_time_cannot_regress_paper_authority() {
        let created_at = Utc::now();
        let aggregate_updated_at = created_at + chrono::Duration::seconds(10);
        let mut paper = paper_with_role_resources(created_at);
        paper.updated_at = aggregate_updated_at;
        let (team, _, evidence_id, _) = team_with_canonical_roles(&paper, created_at);
        let request = CreateRoleResourceActionRequestV1 {
            action_id: Uuid::new_v4(),
            expected_resource_version: 1,
            action: RoleResourceActionInputV1::EvidenceAssessment {
                evidence_card_id: Uuid::new_v4(),
            },
            idempotency_key: "explicit-clock-clamp".into(),
        };

        let action = apply_explicit_role_resource_action(
            &mut paper,
            &team,
            evidence_id,
            &request,
            true,
            created_at,
        )
        .expect("aggregate clock regression is clamped");

        assert_eq!(action.occurred_at, aggregate_updated_at);
        assert_eq!(paper.updated_at, aggregate_updated_at);
    }

    #[test]
    fn observed_clock_regression_cannot_regress_the_replay_ledger() {
        let created_at = Utc::now();
        let mut paper = paper_with_role_resources(created_at);
        let aggregate_updated_at = created_at + chrono::Duration::seconds(10);
        paper.updated_at = aggregate_updated_at;
        let observed_at = created_at - chrono::Duration::seconds(1);
        let run = run_record(
            paper.paper_project_id,
            RunStatus::Cancelled,
            Some("sha256:abababababababababababababababababababababababababababababababab"),
            observed_at,
        );

        let action = apply_run_role_resources(&mut paper, Uuid::new_v4(), &run, observed_at)
            .expect("clock regression is clamped under the aggregate lock")
            .expect("resource action");

        assert_eq!(action.occurred_at, aggregate_updated_at);
        assert_eq!(paper.updated_at, aggregate_updated_at);
        paper
            .role_resources
            .expect("role resources")
            .validate()
            .expect("ledger remains replayable");
    }
}
