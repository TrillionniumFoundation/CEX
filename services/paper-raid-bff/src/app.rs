use std::sync::Arc;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
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
    hepta::{BrowserCommand, HeptaClient},
    html,
    nakama::NakamaArchiveClient,
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
        .route("/alpha/login", post(alpha_login))
        .route("/session/logout", post(logout))
        .route("/session/refresh", post(refresh_session))
        .route("/api/session", get(session_info))
        .route("/api/hepta/commands", post(forward_hepta_command))
        .route("/api/papers/:paper_id/timeline", get(paper_timeline))
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
    Ok(with_cookie(response, issue.cookie))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let _ = state.sessions.consume_csrf(&headers, &session).await?;
    state
        .sessions
        .revoke_subject(&session.identity.subject_id)
        .await?;
    Ok(with_cookie(
        (StatusCode::NO_CONTENT, Body::empty()).into_response(),
        state.sessions.expired_cookie(),
    ))
}

async fn refresh_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let session = state.session(&headers).await?;
    let csrf = state.sessions.refresh_csrf(&headers, &session).await?;
    Ok(Json(json!({"csrf": csrf})))
}

async fn session_info(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let session = state.session(&headers).await?;
    Ok(Json(json!({
        "subject_id": session.identity.subject_id,
        "display_name": session.identity.display_name,
        "player_id": session.identity.player_id,
        "finality": "pending_only"
    })))
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
    with_rotated_csrf(response, next_csrf)
}

async fn lobby(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    Ok(html::lobby(&session.identity))
}

async fn formation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(team_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let team = state.hepta.get_team(&session.identity, team_id).await;
    let acceptances = state
        .hepta
        .get_team_acceptances(&session.identity, team_id)
        .await;
    Ok(html::formation(
        &session.identity,
        &team_id.to_string(),
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
    let paper = state.hepta.get_paper(&session.identity, paper_id).await;
    let submission = state
        .hepta
        .get_submission(&session.identity, paper_id)
        .await;
    Ok(html::paper_room(
        &session.identity,
        &paper_id.to_string(),
        read_state(&paper),
        read_state(&submission),
        html::ReadState::Unavailable,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelineQuery {
    #[serde(default)]
    after_sequence: u64,
}

async fn paper_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Json<Value>, AppError> {
    let session = state.session(&headers).await?;
    let paper = state.hepta.get_paper(&session.identity, paper_id).await?;
    let _ = (paper, query.after_sequence);
    Err(AppError::Unavailable("hepta_p3_room_aggregate_not_rebased"))
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
