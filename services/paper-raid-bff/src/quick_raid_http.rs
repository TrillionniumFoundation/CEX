use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use sha2::Digest;
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    app::{private_no_store, with_rotated_csrf, AppState},
    auth::AuthenticatedSession,
    config::{AlphaIdentity, AlphaIdentityScope},
    error::AppError,
    html,
    quick_raid::{
        QuickRaidActionRequestV1, QuickRaidAuthorityV1, QuickRaidBrowserActionV1,
        QuickRaidConclusionV1, QuickRaidEligibilityV1, QuickRaidError, QuickRaidEventV1,
        QuickRaidEvidenceCardV1, QuickRaidEvidenceChoiceV1, QuickRaidExperimentChoiceV1,
        QuickRaidExperimentRunV1, QuickRaidMetricV1, QuickRaidPaperBundleV1, QuickRaidSessionV1,
        QuickRaidStageV1, QUICK_RAID_ACTION_V1, QUICK_RAID_BRIEF_V1, QUICK_RAID_CHALLENGE_KEY,
        QUICK_RAID_FIXED_SEED, QUICK_RAID_MODE, QUICK_RAID_SCENARIO_V1, QUICK_RAID_SESSION_V1,
    },
};

const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;
const QUICK_RAID_EVIDENCE_CARD_PLAYER_VIEW_V1: &str =
    "hepta.paper_raid.quick_raid_evidence_card_player_view.v1";
const QUICK_RAID_RUN_PLAYER_VIEW_V1: &str = "hepta.paper_raid.quick_raid_run_player_view.v1";
const QUICK_RAID_PAPER_BUNDLE_PLAYER_VIEW_V1: &str =
    "hepta.paper_raid.quick_raid_paper_bundle_player_view.v1";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/league/quick-raid", get(quick_raid_page))
        .route("/api/quick-raid/session", get(quick_raid_session))
        .route("/api/quick-raid/start", post(start_quick_raid))
        .route("/api/quick-raid/action", post(apply_quick_raid_action))
        .route("/api/quick-raid/abandon", post(abandon_quick_raid))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyRequest {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActionRequest {
    expected_version: u64,
    action: QuickRaidBrowserActionV1,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AbandonRequest {
    expected_version: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QuickRaidPlayerViewV1 {
    pub(crate) schema: &'static str,
    pub(crate) mode: &'static str,
    pub(crate) scenario_id: &'static str,
    pub(crate) challenge_key: &'static str,
    pub(crate) seed: u64,
    pub(crate) stage: QuickRaidStageV1,
    pub(crate) version: u64,
    pub(crate) step: u8,
    pub(crate) total_steps: u8,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) remaining_seconds: i64,
    pub(crate) expired: bool,
    pub(crate) terminal: bool,
    /// The domain card is retained here for the server-rendered page, but its
    /// serializer is a bounded player projection (the source digest is not a
    /// gameplay datum and must not cross the player API).
    #[serde(serialize_with = "serialize_evidence_card_player_view")]
    pub(crate) evidence_card: QuickRaidEvidenceCardV1,
    pub(crate) evidence_choice: Option<QuickRaidEvidenceChoiceV1>,
    pub(crate) experiment_choice: Option<QuickRaidExperimentChoiceV1>,
    /// Keep the fully validated run for server-side rendering and persistence;
    /// only its human-readable result and metrics are serialized to players.
    #[serde(serialize_with = "serialize_optional_experiment_run_player_view")]
    pub(crate) experiment_run: Option<QuickRaidExperimentRunV1>,
    pub(crate) conclusion: Option<QuickRaidConclusionV1>,
    /// The internal bundle contains authority, owner/session identifiers and
    /// content hashes.  The player API receives only the visible, non-portable
    /// result preview; the complete bundle remains server/API-authority data.
    #[serde(serialize_with = "serialize_optional_paper_bundle_player_view")]
    pub(crate) paper_bundle: Option<QuickRaidPaperBundleV1>,
    pub(crate) eligibility: QuickRaidEligibilityV1,
}

impl QuickRaidPlayerViewV1 {
    pub(crate) fn from_session(session: &QuickRaidSessionV1, now: DateTime<Utc>) -> Self {
        let clock_expired = !session.stage.is_terminal() && now >= session.expires_at;
        let stage = if clock_expired {
            QuickRaidStageV1::Expired
        } else {
            session.stage
        };
        let step = match stage {
            QuickRaidStageV1::EvidenceReview => 1,
            QuickRaidStageV1::ExperimentRun => 2,
            QuickRaidStageV1::PaperBundleReady => 3,
            QuickRaidStageV1::Completed => 4,
            QuickRaidStageV1::Abandoned | QuickRaidStageV1::Expired => 0,
        };
        Self {
            schema: "hepta.paper_raid.quick_raid_player_view.v1",
            mode: QUICK_RAID_MODE,
            scenario_id: QUICK_RAID_SCENARIO_V1,
            challenge_key: QUICK_RAID_CHALLENGE_KEY,
            seed: QUICK_RAID_FIXED_SEED,
            stage,
            version: session.version,
            step,
            total_steps: 4,
            started_at: session.created_at,
            expires_at: session.expires_at,
            remaining_seconds: (session.expires_at - now).num_seconds().max(0),
            expired: clock_expired || session.stage == QuickRaidStageV1::Expired,
            terminal: stage.is_terminal(),
            evidence_card: session.evidence_card.clone(),
            evidence_choice: session.evidence_choice,
            experiment_choice: session.experiment_choice,
            experiment_run: session.experiment_run.clone(),
            conclusion: session.conclusion,
            paper_bundle: session.paper_bundle.clone(),
            eligibility: session.eligibility.clone(),
        }
    }
}

/// Public Quick Raid card projection.  `QuickRaidEvidenceCardV1` is the
/// validated domain record and includes a source digest used for authority
/// checks; that digest is intentionally not a player-facing field.
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct QuickRaidEvidenceCardPlayerViewV1 {
    schema: String,
    evidence_card_id: String,
    seed: u64,
    claim: String,
    observation: String,
    citation: String,
}

impl From<&QuickRaidEvidenceCardV1> for QuickRaidEvidenceCardPlayerViewV1 {
    fn from(card: &QuickRaidEvidenceCardV1) -> Self {
        Self {
            schema: QUICK_RAID_EVIDENCE_CARD_PLAYER_VIEW_V1.to_owned(),
            evidence_card_id: card.evidence_card_id.clone(),
            seed: card.seed,
            claim: card.claim.clone(),
            observation: card.observation.clone(),
            citation: card.citation.clone(),
        }
    }
}

/// Public run projection.  Run UUIDs and integrity hashes are retained in the
/// internal record and event stream, but are not needed by the browser shell.
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct QuickRaidExperimentRunPlayerViewV1 {
    schema: String,
    seed: u64,
    choice: QuickRaidExperimentChoiceV1,
    result: String,
    metrics: Vec<QuickRaidMetricV1>,
}

impl From<&QuickRaidExperimentRunV1> for QuickRaidExperimentRunPlayerViewV1 {
    fn from(run: &QuickRaidExperimentRunV1) -> Self {
        Self {
            schema: QUICK_RAID_RUN_PLAYER_VIEW_V1.to_owned(),
            seed: run.seed,
            choice: run.choice,
            result: run.result.clone(),
            metrics: run.metrics.clone(),
        }
    }
}

/// Public Paper Bundle preview.  This is deliberately not the authoritative
/// `QuickRaidPaperBundleV1`: no challenge/session UUID, authority material or
/// digest is exposed through the player JSON response.
#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct QuickRaidPaperBundlePlayerViewV1 {
    schema: String,
    seed: u64,
    evidence_card: QuickRaidEvidenceCardPlayerViewV1,
    experiment_run: QuickRaidExperimentRunPlayerViewV1,
    conclusion: QuickRaidConclusionV1,
    finality: String,
    portable: bool,
}

impl From<&QuickRaidPaperBundleV1> for QuickRaidPaperBundlePlayerViewV1 {
    fn from(bundle: &QuickRaidPaperBundleV1) -> Self {
        Self {
            schema: QUICK_RAID_PAPER_BUNDLE_PLAYER_VIEW_V1.to_owned(),
            seed: bundle.seed,
            evidence_card: (&bundle.evidence_card).into(),
            experiment_run: (&bundle.experiment_run).into(),
            conclusion: bundle.conclusion,
            finality: bundle.finality.clone(),
            portable: bundle.portable,
        }
    }
}

fn serialize_evidence_card_player_view<S>(
    value: &QuickRaidEvidenceCardV1,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    QuickRaidEvidenceCardPlayerViewV1::from(value).serialize(serializer)
}

fn serialize_optional_experiment_run_player_view<S>(
    value: &Option<QuickRaidExperimentRunV1>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value
        .as_ref()
        .map(QuickRaidExperimentRunPlayerViewV1::from)
        .serialize(serializer)
}

fn serialize_optional_paper_bundle_player_view<S>(
    value: &Option<QuickRaidPaperBundleV1>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    value
        .as_ref()
        .map(QuickRaidPaperBundlePlayerViewV1::from)
        .serialize(serializer)
}

async fn quick_raid_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    ensure_quick_raid_identity(&session.identity)?;
    if !crate::db::quick_raid_schema_ready(&state.pool).await {
        return Err(AppError::Unavailable("quick_raid_schema_unavailable"));
    }
    let current = load_latest_owned(
        &state.pool,
        &session.identity.subject_id,
        session.identity.player_id,
    )
    .await?;
    let view = current
        .as_ref()
        .map(|session| QuickRaidPlayerViewV1::from_session(session, Utc::now()));
    let binding_ready = resolve_exact_active_binding(&state, &session.identity)
        .await
        .is_ok();
    Ok(html::quick_raid(
        &session.identity,
        view.as_ref(),
        binding_ready,
    ))
}

async fn quick_raid_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    ensure_quick_raid_identity(&session.identity)?;
    if !crate::db::quick_raid_schema_ready(&state.pool).await {
        return Err(AppError::Unavailable("quick_raid_schema_unavailable"));
    }
    let current = load_latest_owned(
        &state.pool,
        &session.identity.subject_id,
        session.identity.player_id,
    )
    .await?;
    Ok(private_no_store(Json(json!({
        "schema": QUICK_RAID_BRIEF_V1,
        "quick_raid": current.as_ref().map(|value| QuickRaidPlayerViewV1::from_session(value, Utc::now())),
    })).into_response()))
}

async fn start_quick_raid(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<EmptyRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        let (quick, created) = start_or_resume(&operation_state, &session).await?;
        let status = if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        Ok((
            status,
            Json(json!({
                "schema": QUICK_RAID_BRIEF_V1,
                "quick_raid": QuickRaidPlayerViewV1::from_session(&quick, Utc::now()),
                "resumed": !created,
            })),
        )
            .into_response())
    })
    .await
}

async fn apply_quick_raid_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ActionRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        let quick = apply_owned_action(&operation_state, &session, request).await?;
        Ok(Json(json!({
            "schema": QUICK_RAID_BRIEF_V1,
            "quick_raid": QuickRaidPlayerViewV1::from_session(&quick, Utc::now()),
        }))
        .into_response())
    })
    .await
}

async fn abandon_quick_raid(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AbandonRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        let quick = apply_owned_action(
            &operation_state,
            &session,
            ActionRequest {
                expected_version: request.expected_version,
                action: QuickRaidBrowserActionV1::Abandon,
            },
        )
        .await?;
        Ok(Json(json!({
            "schema": QUICK_RAID_BRIEF_V1,
            "quick_raid": QuickRaidPlayerViewV1::from_session(&quick, Utc::now()),
        }))
        .into_response())
    })
    .await
}

async fn mutate_with_csrf<F, Fut>(state: &AppState, headers: &HeaderMap, operation: F) -> Response
where
    F: FnOnce(AuthenticatedSession) -> Fut,
    Fut: std::future::Future<Output = Result<Response, AppError>>,
{
    let session = match state.session(headers).await {
        Ok(session) => session,
        Err(error) => return private_no_store(error.into_response()),
    };
    let next_csrf = match state.sessions.consume_csrf(headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return private_no_store(error.into_response()),
    };
    let response = operation(session)
        .await
        .unwrap_or_else(|error| error.into_response());
    with_rotated_csrf(private_no_store(response), next_csrf)
}

fn ensure_quick_raid_identity(identity: &AlphaIdentity) -> Result<(), AppError> {
    if identity.has_scope(AlphaIdentityScope::Author) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn start_or_resume(
    state: &AppState,
    authenticated: &AuthenticatedSession,
) -> Result<(QuickRaidSessionV1, bool), AppError> {
    ensure_quick_raid_identity(&authenticated.identity)?;
    if !crate::db::quick_raid_schema_ready(&state.pool).await {
        return Err(AppError::Unavailable("quick_raid_schema_unavailable"));
    }
    let binding_id = resolve_exact_active_binding(state, &authenticated.identity).await?;
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
    lock_player(&mut transaction, authenticated.identity.player_id).await?;
    if let Some(mut existing) = load_live_for_update(
        &mut transaction,
        &authenticated.identity.subject_id,
        authenticated.identity.player_id,
    )
    .await?
    {
        if existing.binding_id != binding_id {
            return Err(AppError::Conflict("quick_raid_binding_changed".into()));
        }
        if now < existing.expires_at {
            transaction.commit().await?;
            return Ok((existing, false));
        }
        let event = existing
            .expire(
                Uuid::new_v4(),
                digest_label(
                    format!("expire:{}:{}", existing.session_id, existing.version).as_bytes(),
                ),
                now,
            )
            .map_err(map_quick_error)?;
        persist_transition(&mut transaction, &existing, &event).await?;
    }
    let authority = resolve_quick_raid_authority(state, &authenticated.identity).await?;
    let quick = QuickRaidSessionV1::new(
        Uuid::new_v4(),
        authenticated.identity.subject_id.clone(),
        authenticated.identity.player_id,
        binding_id,
        authority,
        now,
    )
    .map_err(map_quick_error)?;
    insert_session(&mut transaction, &quick).await?;
    transaction.commit().await?;
    Ok((quick, true))
}

async fn apply_owned_action(
    state: &AppState,
    authenticated: &AuthenticatedSession,
    request: ActionRequest,
) -> Result<QuickRaidSessionV1, AppError> {
    ensure_quick_raid_identity(&authenticated.identity)?;
    if !crate::db::quick_raid_schema_ready(&state.pool).await {
        return Err(AppError::Unavailable("quick_raid_schema_unavailable"));
    }
    if request.expected_version == 0 || request.expected_version > JSON_SAFE_U64_MAX {
        return Err(AppError::Invalid(
            "quick_raid expected_version is invalid".into(),
        ));
    }
    let abandoning = matches!(request.action, QuickRaidBrowserActionV1::Abandon);
    let current_binding_id = if abandoning {
        None
    } else {
        Some(resolve_exact_active_binding(state, &authenticated.identity).await?)
    };
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
    lock_player(&mut transaction, authenticated.identity.player_id).await?;
    let mut quick = load_live_for_update(
        &mut transaction,
        &authenticated.identity.subject_id,
        authenticated.identity.player_id,
    )
    .await?
    .ok_or(AppError::NotFound)?;
    if current_binding_id.is_some_and(|binding| quick.binding_id != binding) {
        return Err(AppError::Conflict("quick_raid_binding_changed".into()));
    }
    if now >= quick.expires_at {
        let event = quick
            .expire(
                Uuid::new_v4(),
                digest_label(format!("expire:{}:{}", quick.session_id, quick.version).as_bytes()),
                now,
            )
            .map_err(map_quick_error)?;
        persist_transition(&mut transaction, &quick, &event).await?;
        transaction.commit().await?;
        return Err(AppError::Conflict("quick_raid_expired".into()));
    }
    let request_hash = digest_label(
        serde_json::to_vec(&json!({
            "subject_id": authenticated.identity.subject_id,
            "player_id": authenticated.identity.player_id,
            "binding_id": quick.binding_id,
            "session_id": quick.session_id,
            "request": &request,
        }))
        .map_err(|_| AppError::Internal)?
        .as_slice(),
    );
    let action_request = QuickRaidActionRequestV1 {
        schema: QUICK_RAID_ACTION_V1.to_owned(),
        event_id: Uuid::new_v4(),
        session_id: quick.session_id,
        subject_id: authenticated.identity.subject_id.clone(),
        player_id: authenticated.identity.player_id,
        binding_id: quick.binding_id,
        expected_version: request.expected_version,
        request_hash,
        action: request.action,
    };
    let event = quick
        .apply_action(&action_request, now)
        .map_err(map_quick_error)?;
    persist_transition(&mut transaction, &quick, &event).await?;
    transaction.commit().await?;
    Ok(quick)
}

async fn resolve_quick_raid_authority(
    state: &AppState,
    identity: &AlphaIdentity,
) -> Result<QuickRaidAuthorityV1, AppError> {
    let catalog = state.hepta.list_public_challenges(identity).await?;
    let challenges = catalog.as_array().ok_or(AppError::Upstream)?;
    for challenge in challenges {
        let description = challenge
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if description.contains("pack_id=paper-raid-evidence-audit-quick-seeded-v1") {
            return QuickRaidAuthorityV1::from_catalog(challenge).map_err(|error| match error {
                QuickRaidError::ChallengeUnavailable => {
                    AppError::Conflict("quick_raid_pack_unavailable".into())
                }
                _ => AppError::Upstream,
            });
        }
    }
    Err(AppError::Conflict("quick_raid_pack_not_activated".into()))
}

async fn resolve_exact_active_binding(
    state: &AppState,
    identity: &AlphaIdentity,
) -> Result<Uuid, AppError> {
    let value = state.hepta.list_current_agent_bindings(identity).await?;
    let records = value.as_array().ok_or(AppError::Upstream)?;
    let mut active = Vec::new();
    for record in records {
        let owner = parse_uuid(record.get("player_id"))?;
        let binding_id = parse_uuid(record.get("binding_id"))?;
        let status = record
            .get("status")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        if owner != identity.player_id {
            return Err(AppError::Upstream);
        }
        match status {
            "active" => active.push(binding_id),
            "revoked" => {}
            _ => return Err(AppError::Upstream),
        }
    }
    if active.len() != 1 {
        return Err(AppError::Conflict(
            "quick_raid_requires_exactly_one_active_binding".into(),
        ));
    }
    let binding_id = active[0];
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM paper_raid_bff_agent_bridge_bindings WHERE binding_id=$1 AND subject_id=$2 AND player_id=$3",
    )
    .bind(binding_id)
    .bind(&identity.subject_id)
    .bind(identity.player_id)
    .fetch_one(&state.pool)
    .await?;
    if count != 1 {
        return Err(AppError::Conflict(
            "quick_raid_binding_not_paired_locally".into(),
        ));
    }
    Ok(binding_id)
}

fn parse_uuid(value: Option<&Value>) -> Result<Uuid, AppError> {
    let text = value.and_then(Value::as_str).ok_or(AppError::Upstream)?;
    let parsed = Uuid::parse_str(text).map_err(|_| AppError::Upstream)?;
    if parsed.is_nil() || parsed.to_string() != text {
        return Err(AppError::Upstream);
    }
    Ok(parsed)
}

async fn lock_player(
    transaction: &mut Transaction<'_, Postgres>,
    player_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("paper-raid-quick-raid:{player_id}"))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

const SESSION_COLUMNS: &str = "session_id,subject_id,player_id,binding_id,mode,scenario_id,challenge_key,challenge_id,challenge_snapshot_hash,pack_id,ruleset_version,ruleset_hash,authority_hash,seed,duration_seconds,authority_kind,stage,version,evidence_choice,experiment_choice,experiment_run,conclusion,paper_bundle,paper_bundle_hash,activation_eligible,qualification_eligible,scientific_finality_eligible,ranking_eligible,reward_eligible,score_eligible,economic_eligible,completion_portable,created_at,expires_at,updated_at,terminal_at,terminal_reason";

async fn load_latest_owned(
    pool: &PgPool,
    subject_id: &str,
    player_id: Uuid,
) -> Result<Option<QuickRaidSessionV1>, AppError> {
    let query = format!("SELECT {SESSION_COLUMNS} FROM paper_raid_bff_quick_raid_sessions WHERE subject_id=$1 AND player_id=$2 ORDER BY (stage NOT IN ('completed','abandoned','expired')) DESC, created_at DESC LIMIT 1");
    let row = sqlx::query(&query)
        .bind(subject_id)
        .bind(player_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(session_from_row).transpose()
}

async fn load_live_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    subject_id: &str,
    player_id: Uuid,
) -> Result<Option<QuickRaidSessionV1>, AppError> {
    let query = format!("SELECT {SESSION_COLUMNS} FROM paper_raid_bff_quick_raid_sessions WHERE subject_id=$1 AND player_id=$2 AND stage NOT IN ('completed','abandoned','expired') FOR UPDATE");
    let row = sqlx::query(&query)
        .bind(subject_id)
        .bind(player_id)
        .fetch_optional(&mut **transaction)
        .await?;
    row.as_ref().map(session_from_row).transpose()
}

fn session_from_row(row: &PgRow) -> Result<QuickRaidSessionV1, AppError> {
    let authority = QuickRaidAuthorityV1 {
        schema: "hepta.paper_raid.quick_raid_authority.v1".to_owned(),
        authority_kind: row.try_get("authority_kind")?,
        challenge_key: row.try_get("challenge_key")?,
        challenge_id: row.try_get("challenge_id")?,
        challenge_snapshot_hash: row.try_get("challenge_snapshot_hash")?,
        pack_id: row.try_get("pack_id")?,
        ruleset_version: row.try_get("ruleset_version")?,
        ruleset_hash: row.try_get("ruleset_hash")?,
        seed: i64_to_u64(row.try_get("seed")?)?,
        duration_seconds: u32::try_from(row.try_get::<i32, _>("duration_seconds")?)
            .map_err(|_| AppError::Internal)?,
        authority_hash: row.try_get("authority_hash")?,
    };
    let session = QuickRaidSessionV1 {
        schema: QUICK_RAID_SESSION_V1.to_owned(),
        session_id: row.try_get("session_id")?,
        subject_id: row.try_get("subject_id")?,
        player_id: row.try_get("player_id")?,
        binding_id: row.try_get("binding_id")?,
        mode: row.try_get("mode")?,
        scenario_id: row.try_get("scenario_id")?,
        authority,
        eligibility: QuickRaidEligibilityV1 {
            schema: "hepta.paper_raid.quick_raid_eligibility.v1".to_owned(),
            authority_kind: row.try_get("authority_kind")?,
            activation_eligible: row.try_get("activation_eligible")?,
            qualification_eligible: row.try_get("qualification_eligible")?,
            scientific_finality_eligible: row.try_get("scientific_finality_eligible")?,
            ranking_eligible: row.try_get("ranking_eligible")?,
            reward_eligible: row.try_get("reward_eligible")?,
            score_eligible: row.try_get("score_eligible")?,
            economic_eligible: row.try_get("economic_eligible")?,
            completion_portable: row.try_get("completion_portable")?,
        },
        stage: enum_from_text(row.try_get("stage")?)?,
        version: i64_to_u64(row.try_get("version")?)?,
        evidence_card: QuickRaidEvidenceCardV1::fixed(),
        evidence_choice: optional_enum_from_text(row.try_get("evidence_choice")?)?,
        experiment_choice: optional_enum_from_text(row.try_get("experiment_choice")?)?,
        experiment_run: optional_json(row.try_get("experiment_run")?)?,
        conclusion: optional_enum_from_text(row.try_get("conclusion")?)?,
        paper_bundle: optional_json(row.try_get("paper_bundle")?)?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        updated_at: row.try_get("updated_at")?,
        terminal_at: row.try_get("terminal_at")?,
        terminal_reason: optional_enum_from_text(row.try_get("terminal_reason")?)?,
    };
    session.validate().map_err(map_quick_error)?;
    Ok(session)
}

async fn insert_session(
    transaction: &mut Transaction<'_, Postgres>,
    session: &QuickRaidSessionV1,
) -> Result<(), AppError> {
    session.validate().map_err(map_quick_error)?;
    sqlx::query(&format!("INSERT INTO paper_raid_bff_quick_raid_sessions ({SESSION_COLUMNS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35,$36,$37)"))
        .bind(session.session_id).bind(&session.subject_id).bind(session.player_id).bind(session.binding_id)
        .bind(&session.mode).bind(&session.scenario_id).bind(&session.authority.challenge_key)
        .bind(session.authority.challenge_id).bind(&session.authority.challenge_snapshot_hash)
        .bind(&session.authority.pack_id).bind(&session.authority.ruleset_version).bind(&session.authority.ruleset_hash)
        .bind(&session.authority.authority_hash).bind(i64::try_from(session.authority.seed).map_err(|_| AppError::Internal)?).bind(i32::try_from(session.authority.duration_seconds).map_err(|_| AppError::Internal)?)
        .bind(&session.authority.authority_kind).bind(enum_to_text(session.stage)?).bind(i64::try_from(session.version).map_err(|_| AppError::Internal)?)
        .bind(optional_enum_to_text(session.evidence_choice)?).bind(optional_enum_to_text(session.experiment_choice)?)
        .bind(session.experiment_run.as_ref().map(serde_json::to_value).transpose().map_err(|_| AppError::Internal)?)
        .bind(optional_enum_to_text(session.conclusion)?)
        .bind(session.paper_bundle.as_ref().map(serde_json::to_value).transpose().map_err(|_| AppError::Internal)?)
        .bind(session.paper_bundle.as_ref().map(|value| value.paper_bundle_hash.clone()))
        .bind(false).bind(false).bind(false).bind(false).bind(false).bind(false).bind(false).bind(false)
        .bind(session.created_at).bind(session.expires_at).bind(session.updated_at).bind(session.terminal_at)
        .bind(optional_enum_to_text(session.terminal_reason)?).execute(&mut **transaction).await?;
    Ok(())
}

async fn persist_transition(
    transaction: &mut Transaction<'_, Postgres>,
    session: &QuickRaidSessionV1,
    event: &QuickRaidEventV1,
) -> Result<(), AppError> {
    session.validate().map_err(map_quick_error)?;
    let result = sqlx::query("UPDATE paper_raid_bff_quick_raid_sessions SET stage=$2,version=$3,evidence_choice=$4,experiment_choice=$5,experiment_run=$6,conclusion=$7,paper_bundle=$8,paper_bundle_hash=$9,updated_at=$10,terminal_at=$11,terminal_reason=$12 WHERE session_id=$1 AND subject_id=$13 AND player_id=$14 AND binding_id=$15 AND version=$16")
        .bind(session.session_id).bind(enum_to_text(session.stage)?).bind(i64::try_from(session.version).map_err(|_| AppError::Internal)?)
        .bind(optional_enum_to_text(session.evidence_choice)?).bind(optional_enum_to_text(session.experiment_choice)?)
        .bind(session.experiment_run.as_ref().map(serde_json::to_value).transpose().map_err(|_| AppError::Internal)?)
        .bind(optional_enum_to_text(session.conclusion)?)
        .bind(session.paper_bundle.as_ref().map(serde_json::to_value).transpose().map_err(|_| AppError::Internal)?)
        .bind(session.paper_bundle.as_ref().map(|value| value.paper_bundle_hash.clone()))
        .bind(session.updated_at).bind(session.terminal_at).bind(optional_enum_to_text(session.terminal_reason)?)
        .bind(&session.subject_id).bind(session.player_id).bind(session.binding_id)
        .bind(i64::try_from(event.from_version).map_err(|_| AppError::Internal)?).execute(&mut **transaction).await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("quick_raid_version_changed".into()));
    }
    sqlx::query("INSERT INTO paper_raid_bff_quick_raid_events (event_id,session_id,actor_kind,event_kind,from_version,to_version,request_hash,choice_code,result_hash,occurred_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(event.event_id).bind(event.session_id).bind(enum_to_text(event.actor_kind)?).bind(enum_to_text(event.event_kind)?)
        .bind(i64::try_from(event.from_version).map_err(|_| AppError::Internal)?).bind(i64::try_from(event.to_version).map_err(|_| AppError::Internal)?)
        .bind(&event.request_hash).bind(&event.choice_code).bind(&event.result_hash).bind(event.occurred_at).execute(&mut **transaction).await?;
    Ok(())
}

fn i64_to_u64(value: i64) -> Result<u64, AppError> {
    u64::try_from(value).map_err(|_| AppError::Internal)
}
fn optional_json<T: DeserializeOwned>(value: Option<Value>) -> Result<Option<T>, AppError> {
    value
        .map(|value| serde_json::from_value(value).map_err(|_| AppError::Internal))
        .transpose()
}
fn enum_from_text<T: DeserializeOwned>(value: String) -> Result<T, AppError> {
    serde_json::from_value(Value::String(value)).map_err(|_| AppError::Internal)
}
fn optional_enum_from_text<T: DeserializeOwned>(
    value: Option<String>,
) -> Result<Option<T>, AppError> {
    value.map(enum_from_text).transpose()
}
fn enum_to_text<T: Serialize>(value: T) -> Result<String, AppError> {
    serde_json::to_value(value)
        .map_err(|_| AppError::Internal)?
        .as_str()
        .map(str::to_owned)
        .ok_or(AppError::Internal)
}
fn optional_enum_to_text<T: Serialize>(value: Option<T>) -> Result<Option<String>, AppError> {
    value.map(enum_to_text).transpose()
}
fn digest_label(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn map_quick_error(error: QuickRaidError) -> AppError {
    match error {
        QuickRaidError::ChallengeUnavailable => {
            AppError::Conflict("quick_raid_challenge_unavailable".into())
        }
        QuickRaidError::OwnerMismatch => AppError::Forbidden,
        QuickRaidError::StaleVersion => AppError::Conflict("quick_raid_version_changed".into()),
        QuickRaidError::Expired => AppError::Conflict("quick_raid_expired".into()),
        QuickRaidError::Terminal => AppError::Conflict("quick_raid_terminal".into()),
        QuickRaidError::InvalidTransition => {
            AppError::Conflict("quick_raid_invalid_transition".into())
        }
        QuickRaidError::NotExpired => AppError::Conflict("quick_raid_not_expired".into()),
        QuickRaidError::AuthorityEscape
        | QuickRaidError::InvalidContract(_)
        | QuickRaidError::VersionExhausted => AppError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
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
            "description":format!("pack_id={}", crate::quick_raid::QUICK_RAID_PACK_ID),
            "ruleset_version":crate::quick_raid::QUICK_RAID_RULESET_VERSION
        });
        QuickRaidAuthorityV1::from_catalog(&value).expect("quick raid authority fixture")
    }

    fn completed_session() -> QuickRaidSessionV1 {
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
        .expect("quick raid session fixture");
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
            .expect("evidence action");
        session
            .apply_action(
                &action(
                    2,
                    QuickRaidBrowserActionV1::RunExperiment {
                        choice: QuickRaidExperimentChoiceV1::RunCandidate,
                    },
                ),
                now + Duration::seconds(1),
            )
            .expect("experiment action");
        session
            .apply_action(
                &action(
                    3,
                    QuickRaidBrowserActionV1::PublishPaper {
                        conclusion: QuickRaidConclusionV1::RetainWithCaveat,
                    },
                ),
                now + Duration::seconds(2),
            )
            .expect("paper action");
        session
    }

    #[test]
    fn player_view_serializes_only_bounded_public_projection() {
        let now = Utc::now();
        let session = QuickRaidSessionV1::new(
            Uuid::new_v4(),
            "quick-player".into(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            authority(),
            now,
        )
        .expect("quick raid session fixture");
        let encoded = serde_json::to_value(QuickRaidPlayerViewV1::from_session(&session, now))
            .expect("serialize player view");
        let object = encoded.as_object().expect("view object");
        for forbidden in [
            "session_id",
            "subject_id",
            "player_id",
            "binding_id",
            "authority",
            "challenge_id",
            "challenge_snapshot_hash",
            "ruleset_hash",
            "authority_hash",
        ] {
            assert!(!object.contains_key(forbidden), "leaked {forbidden}");
        }
        assert!(encoded["evidence_card"].get("source_digest").is_none());
        assert!(encoded["experiment_run"].is_null());
        assert!(encoded["paper_bundle"].is_null());
    }

    #[test]
    fn completed_player_view_hides_bundle_owner_and_integrity_material() {
        let session = completed_session();
        let encoded = serde_json::to_value(QuickRaidPlayerViewV1::from_session(
            &session,
            session.updated_at,
        ))
        .expect("serialize completed player view");
        let bundle = encoded["paper_bundle"].as_object().expect("bundle preview");
        for forbidden in [
            "authority",
            "session_id",
            "paper_bundle_hash",
            "challenge_id",
            "challenge_snapshot_hash",
            "ruleset_hash",
            "authority_hash",
        ] {
            assert!(
                !bundle.contains_key(forbidden),
                "leaked bundle field {forbidden}"
            );
        }
        assert!(bundle["evidence_card"].get("source_digest").is_none());
        assert!(bundle["experiment_run"].get("run_id").is_none());
        assert!(bundle["experiment_run"].get("run_hash").is_none());
        assert_eq!(bundle["finality"], "none");
        assert_eq!(bundle["portable"], false);
    }
}
