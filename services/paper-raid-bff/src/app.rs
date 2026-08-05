use std::sync::Arc;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::{AuthenticatedSession, SessionStore, CSRF_HEADER},
    cas::CasClient,
    config::{AlphaIdentity, Config, EdgeScope},
    db,
    error::AppError,
    hepta::{BrowserCommand, HeptaClient, HumanRegistrationProof, HumanSigningFrameRequest},
    html,
    nakama::{member_session_accesses, MemberSessionAccess, NakamaArchiveClient},
};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    pool: PgPool,
    sessions: SessionStore,
    hepta: HeptaClient,
    nakama: NakamaArchiveClient,
    cas: CasClient,
}

impl AppState {
    pub async fn connect(config: Config) -> Result<Self, String> {
        let pool = db::connect(&config.database_url)
            .await
            .map_err(|error| format!("cannot connect BFF PostgreSQL: {error}"))?;
        db::migrate(&pool)
            .await
            .map_err(|error| format!("cannot migrate BFF PostgreSQL: {error}"))?;
        let sessions = SessionStore::new(
            pool.clone(),
            &config.session_key,
            config.session_ttl,
            &config.public_origin,
        )?;
        let hepta = HeptaClient::new(
            config.hepta_base.clone(),
            config.assertions.clone(),
            pool.clone(),
        )?;
        let nakama =
            NakamaArchiveClient::new(config.nakama_base.clone(), config.nakama_http_key.clone())?;
        let cas = CasClient::new(config.cas.clone())?;
        Ok(Self {
            config: Arc::new(config),
            pool,
            sessions,
            hepta,
            nakama,
            cas,
        })
    }

    fn identity(&self, subject: &str) -> Option<AlphaIdentity> {
        self.config.identity_for_subject(subject)
    }

    async fn session(&self, headers: &HeaderMap) -> Result<AuthenticatedSession, AppError> {
        self.sessions
            .authenticate(headers, |subject| self.identity(subject))
            .await
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/login", get(login_page))
        .route("/assets/paper-raid.js", get(browser_script))
        .route("/assets/paper-raid.css", get(browser_stylesheet))
        .route("/alpha/login", post(alpha_login))
        .route("/session/logout", post(logout))
        .route("/session/refresh", post(refresh_session))
        .route("/api/session", get(session_info))
        .route("/api/onboarding/human/challenge", post(human_challenge))
        .route("/api/onboarding/human/register", post(register_human))
        .route(
            "/api/onboarding/human/signing-frame",
            post(human_signing_frame),
        )
        .route("/api/hepta/commands", post(forward_hepta_command))
        .route("/api/papers/:paper_id/timeline", get(paper_timeline))
        .route(
            "/api/papers/:paper_id/artifacts/:digest",
            put(upload_artifact).get(download_artifact),
        )
        .route("/league/start", get(league_start))
        .route("/league/onboarding", get(onboarding))
        .route("/league", get(lobby))
        .route("/league/formation/:team_id", get(formation))
        .route("/league/papers/:paper_id", get(paper_room))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state)
}

#[derive(Serialize)]
struct Health<'a> {
    status: &'a str,
    service: &'a str,
}

async fn health() -> Json<Health<'static>> {
    Json(Health {
        status: "ok",
        service: "paper-raid-bff",
    })
}

async fn ready(State(state): State<AppState>) -> Response {
    let (postgres, hepta, nakama, cas) = tokio::join!(
        db::ready(&state.pool),
        state.hepta.ready(),
        state.nakama.ready(),
        state.cas.ready()
    );
    let trust = state.config.identities.len() == 3
        && is_root_url(&state.config.hepta_base)
        && is_root_url(&state.config.nakama_base)
        && is_root_url(&state.config.cas.endpoint)
        && matches!(
            state.config.edge_scope,
            EdgeScope::LoopbackProcess | EdgeScope::ContainerLoopbackPublish
        );
    let ready = postgres && hepta && nakama && cas && trust;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "postgres": postgres,
            "hepta": hepta,
            "nakama": nakama,
            "cas": cas,
            "config_trust": trust,
            "finality": "pending_only"
        })),
    )
        .into_response()
}

async fn login_page() -> Response {
    html::login_page()
}

async fn browser_script() -> Response {
    html::browser_script()
}

async fn browser_stylesheet() -> Response {
    html::browser_stylesheet()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlphaLogin {
    login_key: String,
}

async fn alpha_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AlphaLogin>,
) -> Result<Response, AppError> {
    state.sessions.validate_origin(&headers)?;
    let identity = state
        .config
        .find_identity(&request.login_key)
        .ok_or(AppError::Unauthorized)?;
    let issue = state.sessions.issue(&identity).await?;
    let response = (
        StatusCode::OK,
        Json(json!({
            "subject_id": identity.subject_id,
            "display_name": identity.display_name,
            "player_id": identity.player_id,
            "csrf": issue.csrf
        })),
    )
        .into_response();
    Ok(with_cookie(private_no_store(response), issue.cookie))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let _ = state.sessions.consume_csrf(&headers, &session).await?;
    state
        .sessions
        .revoke_subject(&session.identity.subject_id)
        .await?;
    Ok(with_cookie(
        private_no_store((StatusCode::NO_CONTENT, Body::empty()).into_response()),
        state.sessions.expired_cookie(),
    ))
}

async fn refresh_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let csrf = state.sessions.refresh_csrf(&headers, &session).await?;
    Ok(private_no_store(
        Json(json!({"csrf": csrf})).into_response(),
    ))
}

async fn session_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    Ok(private_no_store(
        Json(json!({
            "subject_id": session.identity.subject_id,
            "display_name": session.identity.display_name,
            "player_id": session.identity.player_id,
            "finality": "pending_only"
        }))
        .into_response(),
    ))
}

async fn league_start(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    match state
        .hepta
        .get_current_human_player(&session.identity)
        .await
    {
        Ok(_) => {
            let bindings = state
                .hepta
                .list_current_agent_bindings(&session.identity)
                .await?;
            if has_active_agent_binding(&bindings, session.identity.player_id)? {
                Ok(Redirect::to("/league").into_response())
            } else {
                Ok(Redirect::to("/league/onboarding").into_response())
            }
        }
        Err(AppError::NotFound) => Ok(Redirect::to("/league/onboarding").into_response()),
        Err(error) => Err(error),
    }
}

async fn onboarding(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    match state
        .hepta
        .get_current_human_player(&session.identity)
        .await
    {
        Err(AppError::NotFound) => Ok(html::onboarding(
            &session.identity,
            html::OnboardingStage::HumanRegistration,
        )),
        Err(_) => Ok(html::onboarding(
            &session.identity,
            html::OnboardingStage::Unavailable,
        )),
        Ok(_) => match state
            .hepta
            .list_current_agent_bindings(&session.identity)
            .await
        {
            Ok(bindings) => match has_active_agent_binding(&bindings, session.identity.player_id) {
                Ok(false) => Ok(html::onboarding(
                    &session.identity,
                    html::OnboardingStage::AgentBinding,
                )),
                Ok(true) => Ok(Redirect::to("/league").into_response()),
                Err(_) => Ok(html::onboarding(
                    &session.identity,
                    html::OnboardingStage::Unavailable,
                )),
            },
            Err(_) => Ok(html::onboarding(
                &session.identity,
                html::OnboardingStage::Unavailable,
            )),
        },
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanChallengeRequest {
    signing_public_key: String,
}

async fn human_challenge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<HumanChallengeRequest>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let response = match HeptaClient::human_registration_challenge(
        &session.identity,
        &request.signing_public_key,
        Utc::now().timestamp(),
    ) {
        Ok(challenge) => Json(challenge).into_response(),
        Err(error) => error.into_response(),
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn register_human(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(proof): Json<HumanRegistrationProof>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let response = match state
        .hepta
        .recover_human_registration(&session.identity, &proof)
        .await
    {
        Ok(Some(player)) => {
            let mut response = Json(player).into_response();
            response.headers_mut().insert(
                "x-paper-raid-registration-recovered",
                HeaderValue::from_static("self-read"),
            );
            response
        }
        Ok(None) => match state
            .hepta
            .create_human_player(&session.identity, &proof)
            .await
        {
            Ok(upstream) => Response::builder()
                .status(upstream.status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(upstream.body))
                .unwrap_or_else(|_| AppError::Internal.into_response()),
            Err(error) => error.into_response(),
        },
        Err(error) => error.into_response(),
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn human_signing_frame(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<HumanSigningFrameRequest>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let response = match state
        .hepta
        .human_signing_frame(&session.identity, &request)
        .await
    {
        Ok(frame) => Json(frame).into_response(),
        Err(error) => error.into_response(),
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn forward_hepta_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(command): Json<BrowserCommand>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let result = state
        .hepta
        .forward_command(&session.identity, &command)
        .await;
    let response = match result {
        Ok(upstream) => {
            let mut response = Response::builder()
                .status(upstream.status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(upstream.body))
                .unwrap_or_else(|_| AppError::Internal.into_response());
            if upstream.replayed {
                response.headers_mut().insert(
                    "x-paper-raid-idempotent-replay",
                    HeaderValue::from_static("true"),
                );
            }
            response
        }
        Err(error) => error.into_response(),
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn lobby(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let (challenges, tickets, proposals, bindings) = tokio::join!(
        state.hepta.list_public_challenges(&session.identity),
        state.hepta.list_matchmaking_tickets(&session.identity),
        state.hepta.list_team_proposals(&session.identity),
        state.hepta.list_current_agent_bindings(&session.identity),
    );
    Ok(html::lobby(
        &session.identity,
        read_state(&challenges),
        read_state(&tickets),
        read_state(&proposals),
        read_state(&bindings),
    ))
}

async fn formation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let (proposal, team, acceptances) = tokio::join!(
        state
            .hepta
            .get_team_proposal(&session.identity, resource_id),
        state.hepta.get_team(&session.identity, resource_id),
        state
            .hepta
            .get_team_acceptances(&session.identity, resource_id),
    );
    Ok(html::formation(
        &session.identity,
        &resource_id.to_string(),
        read_state(&proposal),
        read_state(&team),
        read_state(&acceptances),
    ))
}

async fn paper_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let (room, events, review) = tokio::join!(
        state.hepta.get_paper_room(&session.identity, paper_id),
        state
            .hepta
            .list_paper_room_events(&session.identity, paper_id, 0),
        state
            .hepta
            .get_paper_review_state(&session.identity, paper_id),
    );
    Ok(html::paper_room(
        &session.identity,
        &paper_id.to_string(),
        read_state(&room),
        read_state(&events),
        read_state(&review),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineQuery {
    #[serde(default)]
    after_cursor: u64,
    #[serde(default)]
    after_sequence: u64,
    logical_session_id: Option<String>,
}

async fn paper_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let (room, events) = tokio::join!(
        state.hepta.get_paper_room(&session.identity, paper_id),
        state
            .hepta
            .list_paper_room_events(&session.identity, paper_id, query.after_cursor),
    );
    let room = room?;
    let events = events?;
    let accesses = latest_session_accesses(
        member_session_accesses(&room)?,
        query.logical_session_id.as_deref(),
    )?;
    let mut archives = Vec::with_capacity(accesses.len());
    for access in accesses {
        let archive = state.nakama.archive(&access, query.after_sequence).await?;
        archives.push(json!({
            "logical_session_id": access.logical_session_id,
            "roster_version": access.roster_version,
            "nakama_completion_received": access.nakama_completion_received,
            "archive": archive,
        }));
    }
    Ok(private_no_store(
        Json(json!({
            "paper_id": paper_id,
            "after_cursor": query.after_cursor,
            "after_sequence": query.after_sequence,
            "hepta_events": events,
            "nakama_archives": archives,
            "finality": "pending_only",
        }))
        .into_response(),
    ))
}

async fn upload_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, digest)): Path<(Uuid, String)>,
    body: Body,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let result = upload_artifact_inner(&state, &session, &headers, paper_id, &digest, body).await;
    let response = match result {
        Ok(stored) => (
            if stored.created {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            Json(json!({
                "digest": stored.digest,
                "uri": stored.uri,
                "media_type": stored.media_type,
                "size": stored.size,
                "created": stored.created,
                "acl": "team",
            })),
        )
            .into_response(),
        Err(error) => error.into_response(),
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn upload_artifact_inner(
    state: &AppState,
    session: &AuthenticatedSession,
    headers: &HeaderMap,
    paper_id: Uuid,
    digest: &str,
    body: Body,
) -> Result<crate::cas::StoredObject, AppError> {
    // Membership is checked by Hepta before any request bytes are accepted.
    let _room = state
        .hepta
        .get_paper_room(&session.identity, paper_id)
        .await?;
    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::Invalid("artifact Content-Type is required".into()))?;
    state.cas.validate_media_type(media_type)?;
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > state.cas.max_object_bytes() as u64)
    {
        return Err(AppError::Invalid("artifact body is too large".into()));
    }
    let bytes = collect_limited_body(body, state.cas.max_object_bytes()).await?;
    state.cas.put_if_absent(digest, media_type, &bytes).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactQuery {
    media_type: Option<String>,
}

async fn download_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, digest)): Path<(Uuid, String)>,
    Query(query): Query<ArtifactQuery>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let room = state
        .hepta
        .get_paper_room(&session.identity, paper_id)
        .await?;
    let expected_uri = state.cas.expected_uri(&digest)?;
    let media_type = authorized_artifact_media(&room, &digest, &expected_uri)?;
    if query
        .media_type
        .as_deref()
        .is_some_and(|requested| requested != media_type)
    {
        return Err(AppError::NotFound);
    }
    state.cas.validate_media_type(&media_type)?;
    let bytes = state.cas.get(&digest, &media_type).await?;
    let content_type = HeaderValue::from_str(&media_type).map_err(|_| AppError::Upstream)?;
    let mut response = (StatusCode::OK, Body::from(bytes)).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=paper-raid-artifact"),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'"),
    );
    Ok(response)
}

fn latest_session_accesses(
    accesses: Vec<MemberSessionAccess>,
    logical_session_id: Option<&str>,
) -> Result<Vec<MemberSessionAccess>, AppError> {
    if logical_session_id.is_some_and(|value| {
        value.is_empty()
            || value.len() > 128
            || value.bytes().any(|byte| {
                !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
            })
    }) {
        return Err(AppError::Invalid("logical_session_id is invalid".into()));
    }
    let mut latest: std::collections::BTreeMap<String, MemberSessionAccess> =
        std::collections::BTreeMap::new();
    for access in accesses {
        if logical_session_id.is_some_and(|wanted| wanted != access.logical_session_id) {
            continue;
        }
        match latest.get(&access.logical_session_id) {
            Some(current) if current.roster_version >= access.roster_version => {}
            _ => {
                latest.insert(access.logical_session_id.clone(), access);
            }
        }
    }
    if logical_session_id.is_some() && latest.is_empty() {
        return Err(AppError::NotFound);
    }
    Ok(latest.into_values().collect())
}

fn authorized_artifact_media(
    room: &Value,
    digest: &str,
    expected_uri: &str,
) -> Result<String, AppError> {
    let manifests = room
        .get("artifact_manifests")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let mut authorized: Option<String> = None;
    for manifest in manifests {
        let objects = manifest
            .get("objects")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        let locations = manifest
            .get("storage_locations")
            .and_then(Value::as_array)
            .ok_or(AppError::Upstream)?;
        for object in objects
            .iter()
            .filter(|object| object.get("sha256").and_then(Value::as_str) == Some(digest))
        {
            let logical_path = object
                .get("logical_path")
                .and_then(Value::as_str)
                .ok_or(AppError::Upstream)?;
            let media_type = object
                .get("media_type")
                .and_then(Value::as_str)
                .ok_or(AppError::Upstream)?;
            let matches: Vec<_> = locations
                .iter()
                .filter(|location| {
                    location.get("logical_path").and_then(Value::as_str) == Some(logical_path)
                        && location.get("sha256").and_then(Value::as_str) == Some(digest)
                })
                .collect();
            if matches.len() != 1 {
                return Err(AppError::Upstream);
            }
            let location = matches[0];
            if location.get("uri").and_then(Value::as_str) != Some(expected_uri)
                || !matches!(
                    location.get("acl").and_then(Value::as_str),
                    Some("team" | "reviewers" | "public_after_release")
                )
            {
                return Err(AppError::Forbidden);
            }
            match &authorized {
                Some(existing) if existing != media_type => {
                    return Err(AppError::Conflict(
                        "artifact digest has conflicting media bindings".into(),
                    ))
                }
                Some(_) => {}
                None => authorized = Some(media_type.to_string()),
            }
        }
    }
    authorized.ok_or(AppError::NotFound)
}

async fn collect_limited_body(mut body: Body, limit: usize) -> Result<Vec<u8>, AppError> {
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| AppError::Invalid("artifact body is invalid".into()))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if bytes.len().saturating_add(data.len()) > limit {
            return Err(AppError::Invalid("artifact body is too large".into()));
        }
        bytes.extend_from_slice(&data);
    }
    if bytes.is_empty() {
        return Err(AppError::Invalid("artifact body is empty".into()));
    }
    Ok(bytes)
}

fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

fn with_rotated_csrf(mut response: Response, csrf: String) -> Response {
    if let Ok(value) = HeaderValue::from_str(&csrf) {
        response.headers_mut().insert(CSRF_HEADER, value);
    }
    response
}

fn private_no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn is_root_url(url: &url::Url) -> bool {
    matches!(url.path(), "" | "/")
}

fn read_state(result: &Result<Value, AppError>) -> html::ReadState<'_> {
    match result {
        Ok(value) => html::ReadState::Available(value),
        Err(AppError::NotFound) => html::ReadState::NotFound,
        Err(_) => html::ReadState::Unavailable,
    }
}

fn has_active_agent_binding(value: &Value, player_id: Uuid) -> Result<bool, AppError> {
    let bindings = value.as_array().ok_or(AppError::Upstream)?;
    let mut active = false;
    for binding in bindings {
        let owner = binding
            .get("player_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?;
        let binding_id = binding
            .get("binding_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or(AppError::Upstream)?;
        let agent_id = binding
            .get("agent_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(AppError::Upstream)?;
        let status = binding
            .get("status")
            .and_then(Value::as_str)
            .ok_or(AppError::Upstream)?;
        let _ = (binding_id, agent_id);
        if owner != player_id {
            return Err(AppError::Upstream);
        }
        match status {
            "active" => active = true,
            "revoked" => {}
            _ => return Err(AppError::Upstream),
        }
    }
    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_selects_only_latest_player_scoped_roster() {
        let logical_session_id = "paper.raid:alpha".to_string();
        let accesses = vec![
            MemberSessionAccess {
                logical_session_id: logical_session_id.clone(),
                authorization_id: Uuid::new_v4(),
                roster_version: 1,
                nakama_completion_received: false,
            },
            MemberSessionAccess {
                logical_session_id: logical_session_id.clone(),
                authorization_id: Uuid::new_v4(),
                roster_version: 2,
                nakama_completion_received: true,
            },
            MemberSessionAccess {
                logical_session_id: "paper.raid:other".into(),
                authorization_id: Uuid::new_v4(),
                roster_version: 1,
                nakama_completion_received: false,
            },
        ];
        let selected = latest_session_accesses(accesses.clone(), Some(&logical_session_id))
            .expect("latest scoped access");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].roster_version, 2);
        assert!(selected[0].nakama_completion_received);
        assert!(matches!(
            latest_session_accesses(accesses, Some("not-visible")),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn artifact_download_requires_exact_hepta_manifest_uri_acl_and_media() {
        let digest = format!("sha256:{}", "ab".repeat(32));
        let expected_uri = format!("s3://paper-raid/objects/sha256/{}", "ab".repeat(32));
        let room = json!({
            "artifact_manifests": [{
                "objects": [{
                    "logical_path": "paper/main.md",
                    "sha256": digest,
                    "media_type": "text/markdown"
                }],
                "storage_locations": [{
                    "logical_path": "paper/main.md",
                    "sha256": digest,
                    "uri": expected_uri,
                    "acl": "team"
                }]
            }]
        });
        assert_eq!(
            authorized_artifact_media(&room, &digest, &expected_uri).expect("authorized"),
            "text/markdown"
        );
        let mut tampered = room.clone();
        tampered["artifact_manifests"][0]["storage_locations"][0]["uri"] =
            Value::String("s3://attacker/objects/sha256/deadbeef".into());
        assert!(matches!(
            authorized_artifact_media(&tampered, &digest, &expected_uri),
            Err(AppError::Forbidden)
        ));
        assert!(matches!(
            authorized_artifact_media(&room, &format!("sha256:{}", "cd".repeat(32)), &expected_uri),
            Err(AppError::NotFound)
        ));
    }

    #[tokio::test]
    async fn streaming_artifact_body_enforces_cap_and_rejects_empty() {
        assert_eq!(
            collect_limited_body(Body::from("paper"), 5)
                .await
                .expect("bounded body"),
            b"paper"
        );
        assert!(collect_limited_body(Body::from("oversized"), 4)
            .await
            .is_err());
        assert!(collect_limited_body(Body::empty(), 4).await.is_err());
    }

    #[test]
    fn onboarding_requires_one_self_scoped_active_agent_binding() {
        let player_id = Uuid::new_v4();
        let binding_id = Uuid::new_v4();
        let active = json!([{
            "binding_id": binding_id,
            "player_id": player_id,
            "agent_id": "did:trnm:agent:alpha",
            "status": "active"
        }]);
        assert!(has_active_agent_binding(&active, player_id).expect("active binding"));

        let revoked = json!([{
            "binding_id": binding_id,
            "player_id": player_id,
            "agent_id": "did:trnm:agent:alpha",
            "status": "revoked"
        }]);
        assert!(!has_active_agent_binding(&revoked, player_id).expect("revoked binding"));
        assert!(!has_active_agent_binding(&json!([]), player_id).expect("empty binding list"));

        let foreign = json!([{
            "binding_id": binding_id,
            "player_id": Uuid::new_v4(),
            "agent_id": "did:trnm:agent:alpha",
            "status": "active"
        }]);
        assert!(matches!(
            has_active_agent_binding(&foreign, player_id),
            Err(AppError::Upstream)
        ));
        assert!(matches!(
            has_active_agent_binding(&json!({"bindings": []}), player_id),
            Err(AppError::Upstream)
        ));
    }
}
