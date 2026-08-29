use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    app::{private_no_store, with_rotated_csrf, AppState},
    auth::AuthenticatedSession,
    config::{AlphaIdentity, AlphaIdentityScope},
    error::AppError,
    html,
    practice::{
        PracticeBrowserActionRequestV1, PracticeBrowserActionV1, PracticeEligibilityV1,
        PracticeError, PracticeEventV1, PracticeSessionV1, PracticeStageV1,
        PRACTICE_BROWSER_ACTION_V1, PRACTICE_SESSION_V1,
    },
};

const PRACTICE_PLAYER_VIEW_V1: &str = "hepta.paper_raid.practice_player_view.v1";
const PRACTICE_DURATION_MINUTES: i64 = 20;
const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/league/practice", get(practice_page))
        .route("/api/practice/session", get(practice_session))
        .route("/api/practice/start", post(start_practice))
        .route("/api/practice/advance", post(advance_practice))
        .route("/api/practice/abandon", post(abandon_practice))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartPracticeRequest {}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AdvancePracticeRequest {
    expected_version: u64,
    action: PracticeBrowserActionV1,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AbandonPracticeRequest {
    expected_version: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PracticePlayerViewV1 {
    pub(crate) schema: &'static str,
    pub(crate) mode: String,
    pub(crate) scenario_id: String,
    pub(crate) stage: PracticeStageV1,
    pub(crate) version: u64,
    pub(crate) role: &'static str,
    pub(crate) step: u8,
    pub(crate) total_steps: u8,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) remaining_seconds: i64,
    pub(crate) expired: bool,
    pub(crate) terminal: bool,
    pub(crate) awaiting_agent_bridge: bool,
    pub(crate) eligibility: PracticeEligibilityV1,
}

impl PracticePlayerViewV1 {
    fn from_session(session: &PracticeSessionV1, now: DateTime<Utc>) -> Self {
        let clock_expired = !session.stage.is_terminal() && now >= session.expires_at;
        let stage = if clock_expired {
            PracticeStageV1::Expired
        } else {
            session.stage
        };
        let (role, step) = match stage {
            PracticeStageV1::CaptainPlan => ("captain", 1),
            PracticeStageV1::EvidenceAssessment => ("evidence", 2),
            PracticeStageV1::ExperimentWaitingBridge => ("experiment", 3),
            PracticeStageV1::ExperimentInterpretation => ("experiment", 4),
            PracticeStageV1::CaptainAar => ("captain", 5),
            PracticeStageV1::Completed => ("debrief", 5),
            PracticeStageV1::Abandoned | PracticeStageV1::Expired => ("practice", 0),
        };
        Self {
            schema: PRACTICE_PLAYER_VIEW_V1,
            mode: session.mode.clone(),
            scenario_id: session.scenario_id.clone(),
            stage,
            version: session.version,
            role,
            step,
            total_steps: 5,
            started_at: session.created_at,
            expires_at: session.expires_at,
            remaining_seconds: (session.expires_at - now).num_seconds().max(0),
            expired: clock_expired || session.stage == PracticeStageV1::Expired,
            terminal: stage.is_terminal(),
            awaiting_agent_bridge: stage == PracticeStageV1::ExperimentWaitingBridge,
            eligibility: session.eligibility.clone(),
        }
    }
}

async fn practice_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    ensure_practice_identity(&session.identity)?;
    let practice = load_latest_owned_session(
        &state.pool,
        &session.identity.subject_id,
        session.identity.player_id,
    )
    .await?;
    let view = practice
        .as_ref()
        .map(|practice| PracticePlayerViewV1::from_session(practice, Utc::now()));
    let binding_ready = resolve_exact_active_binding(&state, &session.identity)
        .await
        .is_ok();
    Ok(html::practice(
        &session.identity,
        view.as_ref(),
        binding_ready,
    ))
}

async fn practice_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    ensure_practice_identity(&session.identity)?;
    let practice = load_latest_owned_session(
        &state.pool,
        &session.identity.subject_id,
        session.identity.player_id,
    )
    .await?;
    let view = practice
        .as_ref()
        .map(|practice| PracticePlayerViewV1::from_session(practice, Utc::now()));
    Ok(private_no_store(
        Json(json!({ "practice": view })).into_response(),
    ))
}

async fn start_practice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<StartPracticeRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        let (practice, created) = start_or_resume(&operation_state, &session).await?;
        let status = if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        };
        Ok((
            status,
            Json(json!({
                "practice": PracticePlayerViewV1::from_session(&practice, Utc::now()),
                "resumed": !created,
            })),
        )
            .into_response())
    })
    .await
}

async fn advance_practice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AdvancePracticeRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        if matches!(&request.action, PracticeBrowserActionV1::Abandon) {
            return Err(AppError::Invalid(
                "abandon must use the dedicated practice endpoint".into(),
            ));
        }
        let practice = apply_owned_browser_action(&operation_state, &session, request).await?;
        Ok(Json(json!({
            "practice": PracticePlayerViewV1::from_session(&practice, Utc::now()),
        }))
        .into_response())
    })
    .await
}

async fn abandon_practice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AbandonPracticeRequest>,
) -> Response {
    let operation_state = state.clone();
    mutate_with_csrf(&state, &headers, move |session| async move {
        let practice = apply_owned_browser_action(
            &operation_state,
            &session,
            AdvancePracticeRequest {
                expected_version: request.expected_version,
                action: PracticeBrowserActionV1::Abandon,
            },
        )
        .await?;
        Ok(Json(json!({
            "practice": PracticePlayerViewV1::from_session(&practice, Utc::now()),
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

fn ensure_practice_identity(identity: &AlphaIdentity) -> Result<(), AppError> {
    if identity.has_scope(AlphaIdentityScope::Author)
        || identity.has_scope(AlphaIdentityScope::Evaluator)
        || identity.has_scope(AlphaIdentityScope::Reviewer)
        || identity.has_scope(AlphaIdentityScope::Reproducer)
    {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn start_or_resume(
    state: &AppState,
    authenticated: &AuthenticatedSession,
) -> Result<(PracticeSessionV1, bool), AppError> {
    ensure_practice_identity(&authenticated.identity)?;
    let binding_id = resolve_exact_active_binding(state, &authenticated.identity).await?;
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
    lock_player(&mut transaction, authenticated.identity.player_id).await?;
    if let Some(mut existing) = load_live_owned_session_for_update(
        &mut transaction,
        &authenticated.identity.subject_id,
        authenticated.identity.player_id,
    )
    .await?
    {
        if existing.binding_id != binding_id {
            return Err(AppError::Conflict("practice_binding_changed".to_string()));
        }
        if now < existing.expires_at {
            transaction.commit().await?;
            return Ok((existing, false));
        }
        let event = existing
            .expire(
                Uuid::new_v4(),
                digest_label(
                    format!(
                        "expire:{}:{}:{}",
                        existing.practice_session_id, existing.version, existing.expires_at
                    )
                    .as_bytes(),
                ),
                now,
            )
            .map_err(map_practice_error)?;
        persist_transition(&mut transaction, &existing, &event).await?;
    }

    let practice = PracticeSessionV1::new(
        Uuid::new_v4(),
        authenticated.identity.subject_id.clone(),
        authenticated.identity.player_id,
        binding_id,
        Uuid::new_v4(),
        now,
        now + Duration::minutes(PRACTICE_DURATION_MINUTES),
    )
    .map_err(map_practice_error)?;
    insert_session(&mut transaction, &practice).await?;
    transaction.commit().await?;
    Ok((practice, true))
}

async fn apply_owned_browser_action(
    state: &AppState,
    authenticated: &AuthenticatedSession,
    request: AdvancePracticeRequest,
) -> Result<PracticeSessionV1, AppError> {
    ensure_practice_identity(&authenticated.identity)?;
    if request.expected_version == 0 || request.expected_version > JSON_SAFE_U64_MAX {
        return Err(AppError::Invalid(
            "practice expected_version is invalid".into(),
        ));
    }
    let abandoning = matches!(&request.action, PracticeBrowserActionV1::Abandon);
    let current_binding_id = if abandoning {
        None
    } else {
        Some(resolve_exact_active_binding(state, &authenticated.identity).await?)
    };
    apply_stored_browser_action(
        &state.pool,
        &authenticated.identity,
        current_binding_id,
        request,
    )
    .await
}

async fn apply_stored_browser_action(
    pool: &PgPool,
    identity: &AlphaIdentity,
    current_binding_id: Option<Uuid>,
    request: AdvancePracticeRequest,
) -> Result<PracticeSessionV1, AppError> {
    let abandoning = matches!(&request.action, PracticeBrowserActionV1::Abandon);
    if abandoning != current_binding_id.is_none() {
        return Err(AppError::Internal);
    }
    let now = Utc::now();
    let mut transaction = pool.begin().await?;
    lock_player(&mut transaction, identity.player_id).await?;
    let mut practice = load_live_owned_session_for_update(
        &mut transaction,
        &identity.subject_id,
        identity.player_id,
    )
    .await?
    .ok_or(AppError::NotFound)?;
    if current_binding_id.is_some_and(|binding_id| practice.binding_id != binding_id) {
        return Err(AppError::Conflict("practice_binding_changed".to_string()));
    }
    if now >= practice.expires_at {
        let event = practice
            .expire(
                Uuid::new_v4(),
                digest_label(
                    format!(
                        "expire:{}:{}:{}",
                        practice.practice_session_id, practice.version, practice.expires_at
                    )
                    .as_bytes(),
                ),
                now,
            )
            .map_err(map_practice_error)?;
        persist_transition(&mut transaction, &practice, &event).await?;
        transaction.commit().await?;
        return Err(AppError::Conflict("practice_expired".to_string()));
    }

    let request_hash = digest_label(
        serde_json::to_vec(&json!({
            "subject_id": identity.subject_id,
            "player_id": identity.player_id,
            "binding_id": practice.binding_id,
            "practice_session_id": practice.practice_session_id,
            "request": &request,
        }))
        .map_err(|_| AppError::Internal)?
        .as_slice(),
    );
    let action = PracticeBrowserActionRequestV1 {
        schema: PRACTICE_BROWSER_ACTION_V1.to_owned(),
        event_id: Uuid::new_v4(),
        practice_session_id: practice.practice_session_id,
        subject_id: identity.subject_id.clone(),
        player_id: identity.player_id,
        binding_id: practice.binding_id,
        expected_version: request.expected_version,
        request_hash,
        action: request.action,
    };
    let event = practice
        .apply_browser_action(&action, now)
        .map_err(map_practice_error)?;
    persist_transition(&mut transaction, &practice, &event).await?;
    transaction.commit().await?;
    Ok(practice)
}

async fn resolve_exact_active_binding(
    state: &AppState,
    identity: &AlphaIdentity,
) -> Result<Uuid, AppError> {
    let value = state.hepta.list_current_agent_bindings(identity).await?;
    let records = value.as_array().ok_or(AppError::Upstream)?;
    let mut active = Vec::new();
    for record in records {
        let owner = parse_canonical_uuid(record.get("player_id"))?;
        let binding_id = parse_canonical_uuid(record.get("binding_id"))?;
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
            "practice_requires_exactly_one_active_binding".into(),
        ));
    }
    let binding_id = active[0];
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM paper_raid_bff_agent_bridge_bindings \
         WHERE binding_id=$1 AND subject_id=$2 AND player_id=$3",
    )
    .bind(binding_id)
    .bind(&identity.subject_id)
    .bind(identity.player_id)
    .fetch_one(&state.pool)
    .await?;
    if count != 1 {
        return Err(AppError::Conflict(
            "practice_binding_not_paired_locally".into(),
        ));
    }
    Ok(binding_id)
}

fn parse_canonical_uuid(value: Option<&Value>) -> Result<Uuid, AppError> {
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
        .bind(format!("paper-raid-practice:{player_id}"))
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

const SESSION_COLUMNS: &str = "practice_session_id,subject_id,player_id,binding_id,mode,scenario_id,authority_kind,stage,version,captain_plan,evidence_assessment,bridge_task_id,bridge_task_state,bridge_result_code,bridge_result_hash,experiment_interpretation,aar_choice,activation_eligible,qualification_eligible,scientific_finality_eligible,ranking_eligible,reward_eligible,score_eligible,economic_eligible,completion_portable,created_at,expires_at,updated_at,terminal_at,terminal_reason";

async fn load_latest_owned_session(
    pool: &PgPool,
    subject_id: &str,
    player_id: Uuid,
) -> Result<Option<PracticeSessionV1>, AppError> {
    let query = format!(
        "SELECT {SESSION_COLUMNS} FROM paper_raid_bff_practice_sessions \
         WHERE subject_id=$1 AND player_id=$2 \
         ORDER BY (stage NOT IN ('completed','abandoned','expired')) DESC, created_at DESC \
         LIMIT 1"
    );
    let row = sqlx::query(&query)
        .bind(subject_id)
        .bind(player_id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(session_from_row).transpose()
}

async fn load_live_owned_session_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    subject_id: &str,
    player_id: Uuid,
) -> Result<Option<PracticeSessionV1>, AppError> {
    let query = format!(
        "SELECT {SESSION_COLUMNS} FROM paper_raid_bff_practice_sessions \
         WHERE subject_id=$1 AND player_id=$2 \
           AND stage NOT IN ('completed','abandoned','expired') \
         FOR UPDATE"
    );
    let row = sqlx::query(&query)
        .bind(subject_id)
        .bind(player_id)
        .fetch_optional(&mut **transaction)
        .await?;
    row.as_ref().map(session_from_row).transpose()
}

fn session_from_row(row: &PgRow) -> Result<PracticeSessionV1, AppError> {
    let eligibility = PracticeEligibilityV1 {
        schema: crate::practice::PRACTICE_ELIGIBILITY_V1.to_owned(),
        authority_kind: row.try_get("authority_kind")?,
        activation_eligible: row.try_get("activation_eligible")?,
        qualification_eligible: row.try_get("qualification_eligible")?,
        scientific_finality_eligible: row.try_get("scientific_finality_eligible")?,
        ranking_eligible: row.try_get("ranking_eligible")?,
        reward_eligible: row.try_get("reward_eligible")?,
        score_eligible: row.try_get("score_eligible")?,
        economic_eligible: row.try_get("economic_eligible")?,
        completion_portable: row.try_get("completion_portable")?,
    };
    let session = PracticeSessionV1 {
        schema: PRACTICE_SESSION_V1.to_owned(),
        practice_session_id: row.try_get("practice_session_id")?,
        subject_id: row.try_get("subject_id")?,
        player_id: row.try_get("player_id")?,
        binding_id: row.try_get("binding_id")?,
        mode: row.try_get("mode")?,
        scenario_id: row.try_get("scenario_id")?,
        eligibility,
        stage: enum_from_text(row.try_get("stage")?)?,
        version: i64_to_version(row.try_get("version")?)?,
        captain_plan: optional_enum_from_text(row.try_get("captain_plan")?)?,
        evidence_assessment: optional_enum_from_text(row.try_get("evidence_assessment")?)?,
        bridge_task_id: row.try_get("bridge_task_id")?,
        bridge_task_state: enum_from_text(row.try_get("bridge_task_state")?)?,
        bridge_result_code: optional_enum_from_text(row.try_get("bridge_result_code")?)?,
        bridge_result_hash: row.try_get("bridge_result_hash")?,
        experiment_interpretation: optional_enum_from_text(
            row.try_get("experiment_interpretation")?,
        )?,
        aar_choice: optional_enum_from_text(row.try_get("aar_choice")?)?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        updated_at: row.try_get("updated_at")?,
        terminal_at: row.try_get("terminal_at")?,
        terminal_reason: optional_enum_from_text(row.try_get("terminal_reason")?)?,
    };
    session.validate().map_err(map_practice_error)?;
    Ok(session)
}

async fn insert_session(
    transaction: &mut Transaction<'_, Postgres>,
    session: &PracticeSessionV1,
) -> Result<(), AppError> {
    session.validate().map_err(map_practice_error)?;
    sqlx::query(
        "INSERT INTO paper_raid_bff_practice_sessions ( \
            practice_session_id,subject_id,player_id,binding_id,mode,scenario_id,authority_kind, \
            stage,version,captain_plan,evidence_assessment,bridge_task_id,bridge_task_state, \
            bridge_result_code,bridge_result_hash,experiment_interpretation,aar_choice, \
            activation_eligible,qualification_eligible,scientific_finality_eligible, \
            ranking_eligible,reward_eligible,score_eligible,economic_eligible,completion_portable, \
            created_at,expires_at,updated_at,terminal_at,terminal_reason \
         ) VALUES ( \
            $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17, \
            $18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30 \
         )",
    )
    .bind(session.practice_session_id)
    .bind(&session.subject_id)
    .bind(session.player_id)
    .bind(session.binding_id)
    .bind(&session.mode)
    .bind(&session.scenario_id)
    .bind(&session.eligibility.authority_kind)
    .bind(enum_to_text(session.stage)?)
    .bind(version_to_i64(session.version)?)
    .bind(optional_enum_to_text(session.captain_plan)?)
    .bind(optional_enum_to_text(session.evidence_assessment)?)
    .bind(session.bridge_task_id)
    .bind(enum_to_text(session.bridge_task_state)?)
    .bind(optional_enum_to_text(session.bridge_result_code)?)
    .bind(&session.bridge_result_hash)
    .bind(optional_enum_to_text(session.experiment_interpretation)?)
    .bind(optional_enum_to_text(session.aar_choice)?)
    .bind(session.eligibility.activation_eligible)
    .bind(session.eligibility.qualification_eligible)
    .bind(session.eligibility.scientific_finality_eligible)
    .bind(session.eligibility.ranking_eligible)
    .bind(session.eligibility.reward_eligible)
    .bind(session.eligibility.score_eligible)
    .bind(session.eligibility.economic_eligible)
    .bind(session.eligibility.completion_portable)
    .bind(session.created_at)
    .bind(session.expires_at)
    .bind(session.updated_at)
    .bind(session.terminal_at)
    .bind(optional_enum_to_text(session.terminal_reason)?)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_transition(
    transaction: &mut Transaction<'_, Postgres>,
    session: &PracticeSessionV1,
    event: &PracticeEventV1,
) -> Result<(), AppError> {
    session.validate().map_err(map_practice_error)?;
    event.validate().map_err(map_practice_error)?;
    let result = sqlx::query(
        "UPDATE paper_raid_bff_practice_sessions SET \
            stage=$2,version=$3,captain_plan=$4,evidence_assessment=$5, \
            bridge_task_state=$6,bridge_result_code=$7,bridge_result_hash=$8, \
            experiment_interpretation=$9,aar_choice=$10,updated_at=$11, \
            terminal_at=$12,terminal_reason=$13 \
         WHERE practice_session_id=$1 AND subject_id=$14 AND player_id=$15 \
           AND binding_id=$16 AND version=$17",
    )
    .bind(session.practice_session_id)
    .bind(enum_to_text(session.stage)?)
    .bind(version_to_i64(session.version)?)
    .bind(optional_enum_to_text(session.captain_plan)?)
    .bind(optional_enum_to_text(session.evidence_assessment)?)
    .bind(enum_to_text(session.bridge_task_state)?)
    .bind(optional_enum_to_text(session.bridge_result_code)?)
    .bind(&session.bridge_result_hash)
    .bind(optional_enum_to_text(session.experiment_interpretation)?)
    .bind(optional_enum_to_text(session.aar_choice)?)
    .bind(session.updated_at)
    .bind(session.terminal_at)
    .bind(optional_enum_to_text(session.terminal_reason)?)
    .bind(&session.subject_id)
    .bind(session.player_id)
    .bind(session.binding_id)
    .bind(version_to_i64(event.from_version)?)
    .execute(&mut **transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("practice_version_changed".into()));
    }
    sqlx::query(
        "INSERT INTO paper_raid_bff_practice_events ( \
            event_id,practice_session_id,actor_kind,event_kind,from_version,to_version, \
            request_hash,choice_code,result_hash,occurred_at \
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(event.event_id)
    .bind(event.practice_session_id)
    .bind(enum_to_text(event.actor_kind)?)
    .bind(enum_to_text(event.event_kind)?)
    .bind(version_to_i64(event.from_version)?)
    .bind(version_to_i64(event.to_version)?)
    .bind(&event.request_hash)
    .bind(&event.choice_code)
    .bind(&event.result_hash)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
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

fn i64_to_version(value: i64) -> Result<u64, AppError> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0 && *value <= JSON_SAFE_U64_MAX)
        .ok_or(AppError::Internal)
}

fn version_to_i64(value: u64) -> Result<i64, AppError> {
    if value == 0 || value > JSON_SAFE_U64_MAX {
        return Err(AppError::Internal);
    }
    i64::try_from(value).map_err(|_| AppError::Internal)
}

fn digest_label(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn map_practice_error(error: PracticeError) -> AppError {
    match error {
        PracticeError::OwnerMismatch
        | PracticeError::SessionMismatch
        | PracticeError::TaskMismatch
        | PracticeError::AuthorityEscape => AppError::Forbidden,
        PracticeError::StaleVersion => AppError::Conflict("practice_version_changed".into()),
        PracticeError::Expired | PracticeError::NotExpired => {
            AppError::Conflict("practice_expired".into())
        }
        PracticeError::Terminal | PracticeError::InvalidTransition => {
            AppError::Conflict("practice_transition_rejected".into())
        }
        PracticeError::InvalidContract(_) | PracticeError::VersionExhausted => AppError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::practice::CaptainPlanChoiceV1;

    async fn insert_paired_binding_fixture(
        pool: &PgPool,
        subject_id: &str,
        player_id: Uuid,
        binding_id: Uuid,
        now: DateTime<Utc>,
    ) {
        let grant_id = Uuid::new_v4();
        let agent_id = format!("practice-http-agent-{}", Uuid::new_v4());
        let agent_key_id = digest_label(b"practice-http-agent-key");
        let capability_disclosure_hash = digest_label(b"practice-http-capability-disclosure");
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
        sqlx::query(
            "INSERT INTO paper_raid_bff_agent_pairing_grants ( \
                grant_id, subject_id, player_id, code_hash, state, \
                pinned_request_hash, pinned_binding_id, created_at, expires_at, \
                pinned_at, consumed_at, pair_response_status, pair_response_body, updated_at \
             ) VALUES ($1,$2,$3,$4,'consumed',$5,$6,$7,$8,$7,$7,200,$9,$7)",
        )
        .bind(grant_id)
        .bind(subject_id)
        .bind(player_id)
        .bind(Sha256::digest(Uuid::new_v4().as_bytes()).to_vec())
        .bind(Sha256::digest(Uuid::new_v4().as_bytes()).to_vec())
        .bind(binding_id)
        .bind(now)
        .bind(now + Duration::minutes(4))
        .bind(b"{}".as_slice())
        .execute(pool)
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
        .bind(subject_id)
        .bind(player_id)
        .bind(&agent_id)
        .bind(&agent_key_id)
        .bind(&capability_disclosure_hash)
        .bind(&capability_disclosure)
        .bind(&binding_record)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert paired binding fixture");
    }

    #[test]
    fn player_requests_reject_owner_authority_and_cross_domain_fields() {
        let valid = json!({
            "expected_version": 1,
            "action": {
                "action": "captain_plan",
                "choice": "audit_highest_risk_claim",
            }
        });
        serde_json::from_value::<AdvancePracticeRequest>(valid.clone())
            .expect("bounded player request");
        for forbidden in [
            "practice_session_id",
            "subject_id",
            "player_id",
            "binding_id",
            "event_id",
            "request_hash",
            "paper_project_id",
            "challenge_id",
            "activation_id",
            "qualification_id",
            "finality_receipt_hash",
            "rank",
            "reward",
            "economy",
        ] {
            let mut hostile = valid.clone();
            hostile[forbidden] = json!(Uuid::new_v4());
            assert!(
                serde_json::from_value::<AdvancePracticeRequest>(hostile).is_err(),
                "accepted hostile field {forbidden}"
            );
        }
    }

    #[test]
    fn player_action_shape_and_abandon_request_are_exact() {
        assert!(serde_json::from_value::<AdvancePracticeRequest>(json!({
            "expected_version": 1,
            "action": {"action": "captain_plan"}
        }))
        .is_err());
        assert!(serde_json::from_value::<AdvancePracticeRequest>(json!({
            "expected_version": 1,
            "action": {"action": "captain_plan", "choice": "unsupported_claim"}
        }))
        .is_err());
        assert!(serde_json::from_value::<AbandonPracticeRequest>(json!({
            "expected_version": 1,
            "choice": "anything"
        }))
        .is_err());
    }

    #[test]
    fn player_view_never_exposes_owner_task_or_hash_fields() {
        let now = Utc::now();
        let session = PracticeSessionV1::new(
            Uuid::new_v4(),
            "practice-owner".into(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now + Duration::minutes(20),
        )
        .expect("practice fixture");
        let encoded = serde_json::to_value(PracticePlayerViewV1::from_session(&session, now))
            .expect("serialize player view");
        let object = encoded.as_object().expect("view object");
        for forbidden in [
            "practice_session_id",
            "subject_id",
            "player_id",
            "binding_id",
            "bridge_task_id",
            "request_hash",
            "result_hash",
        ] {
            assert!(!object.contains_key(forbidden), "leaked {forbidden}");
        }
        assert_eq!(encoded["eligibility"]["ranking_eligible"], false);
        assert_eq!(encoded["eligibility"]["reward_eligible"], false);
        assert_eq!(
            encoded["eligibility"]["scientific_finality_eligible"],
            false
        );
    }

    #[test]
    fn enum_and_version_database_round_trips_are_exact() {
        assert_eq!(
            enum_to_text(PracticeStageV1::ExperimentWaitingBridge).unwrap(),
            "experiment_waiting_bridge"
        );
        assert_eq!(
            enum_from_text::<CaptainPlanChoiceV1>("audit_evidence_chain_first".into()).unwrap(),
            CaptainPlanChoiceV1::AuditEvidenceChainFirst
        );
        assert!(enum_from_text::<PracticeStageV1>("paper_finality".into()).is_err());
        assert_eq!(i64_to_version(1).unwrap(), 1);
        assert!(i64_to_version(0).is_err());
        assert!(version_to_i64(JSON_SAFE_U64_MAX + 1).is_err());
    }

    #[tokio::test]
    async fn real_postgres_abandon_uses_stored_binding_after_external_revocation() {
        let Ok(database_url) = std::env::var("PAPER_RAID_BFF_TEST_DATABASE_URL") else {
            eprintln!(
                "PAPER_RAID_BFF_TEST_DATABASE_URL is unset; practice abandon PG gate skipped"
            );
            return;
        };
        let pool = crate::db::connect(&database_url)
            .await
            .expect("connect practice abandon PostgreSQL");
        crate::db::migrate(&pool)
            .await
            .expect("migrate practice abandon PostgreSQL");

        let now = Utc::now();
        let subject_id = format!("practice-http-subject-{}", Uuid::new_v4());
        let player_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        insert_paired_binding_fixture(&pool, &subject_id, player_id, binding_id, now).await;
        let identity = AlphaIdentity::test_identity(&subject_id, player_id, Uuid::new_v4());
        let practice = PracticeSessionV1::new(
            Uuid::new_v4(),
            subject_id,
            player_id,
            binding_id,
            Uuid::new_v4(),
            now,
            now + Duration::minutes(PRACTICE_DURATION_MINUTES),
        )
        .expect("valid practice session fixture");
        let mut transaction = pool.begin().await.expect("begin practice fixture");
        insert_session(&mut transaction, &practice)
            .await
            .expect("insert practice fixture");
        transaction.commit().await.expect("commit practice fixture");

        let abandoned = apply_stored_browser_action(
            &pool,
            &identity,
            None,
            AdvancePracticeRequest {
                expected_version: practice.version,
                action: PracticeBrowserActionV1::Abandon,
            },
        )
        .await
        .expect("abandon must not require a still-active external binding");
        assert_eq!(abandoned.stage, PracticeStageV1::Abandoned);
        assert_eq!(abandoned.binding_id, binding_id);
        assert_eq!(abandoned.version, practice.version + 1);

        let stored = load_latest_owned_session(&pool, &identity.subject_id, identity.player_id)
            .await
            .expect("read abandoned practice")
            .expect("abandoned practice row");
        assert_eq!(stored.stage, PracticeStageV1::Abandoned);
        assert_eq!(stored.binding_id, binding_id);
        let event = sqlx::query(
            "SELECT actor_kind,event_kind,from_version,to_version \
             FROM paper_raid_bff_practice_events \
             WHERE practice_session_id=$1",
        )
        .bind(practice.practice_session_id)
        .fetch_one(&pool)
        .await
        .expect("read abandon event");
        assert_eq!(event.get::<String, _>("actor_kind"), "browser");
        assert_eq!(event.get::<String, _>("event_kind"), "abandoned");
        assert_eq!(event.get::<i64, _>("from_version"), 1);
        assert_eq!(event.get::<i64, _>("to_version"), 2);
    }
}
