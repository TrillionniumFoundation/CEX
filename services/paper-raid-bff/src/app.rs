use std::{
    collections::{BTreeMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use axum::{
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use hepta_paper_raid_contracts::{verify_frozen_review_bundle, FrozenReviewBundleV1};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    access::AccessDirectory,
    agent_bridge,
    auth::{AuthenticatedSession, SessionStore, CSRF_HEADER},
    cas::CasClient,
    config::{AlphaAuthorRole, AlphaIdentity, AlphaIdentityScope, Config, EdgeScope, IdentityMode},
    db,
    error::AppError,
    hepta::{
        matchmaking_party_payload_is_safe, AuthenticatedPaperReviewState, AuthenticatedPaperRoom,
        BrowserCommand, CommandName, HeptaClient, HeptaPaperTerminalOutcome,
        HeptaPaperTerminalReason, HumanRegistrationProof, HumanSigningFrameRequest,
    },
    html,
    metrics::{self, InviteOutcome, Metrics},
    nakama::{member_session_accesses, MemberSessionAccess, NakamaArchiveClient},
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) config: Arc<Config>,
    pub(crate) pool: PgPool,
    pub(crate) sessions: SessionStore,
    pub(crate) access: Option<AccessDirectory>,
    pub(crate) hepta: HeptaClient,
    pub(crate) metrics: Metrics,
    nakama: NakamaArchiveClient,
    pub(crate) cas: CasClient,
}

impl AppState {
    pub async fn connect(config: Config) -> Result<Self, String> {
        let pool = db::connect(&config.database_url)
            .await
            .map_err(|error| format!("cannot connect BFF PostgreSQL: {error}"))?;
        match config.identity_mode {
            IdentityMode::FixedAlpha => db::migrate(&pool)
                .await
                .map_err(|error| format!("cannot migrate BFF PostgreSQL: {error}"))?,
            IdentityMode::InviteAlpha if db::invite_schema_ready(&pool).await => {
                let invite = config
                    .invite_alpha
                    .as_ref()
                    .ok_or_else(|| "invite-alpha activation pins are absent".to_string())?;
                if !db::invite_activation_ready(&pool, invite).await {
                    return Err(
                        "invite-alpha activation authority is absent, expired, revoked, pin-mismatched, or runtime ACL drifted"
                            .to_string(),
                    );
                }
            }
            IdentityMode::InviteAlpha => {
                return Err(
                    "invite-alpha schema is not provisioned; run paper-raid-accessctl with the separate operator DSN before starting the BFF"
                        .to_string(),
                )
            }
        }
        let metrics = Metrics::default();
        if let Some(snapshot) = db::load_metrics_snapshot(&pool)
            .await
            .map_err(|error| format!("cannot load durable BFF metrics snapshot: {error}"))?
        {
            metrics
                .restore_snapshot(&snapshot)
                .map_err(|error| format!("cannot restore durable BFF metrics snapshot: {error}"))?;
        }
        let (initial_snapshot, initial_revision) = metrics
            .snapshot_with_revision()
            .map_err(|error| format!("cannot encode durable BFF metrics snapshot: {error}"))?;
        if !db::persist_metrics_snapshot(&pool, &initial_snapshot)
            .await
            .map_err(|error| format!("cannot persist initial BFF metrics snapshot: {error}"))?
        {
            return Err("durable BFF metrics snapshot is newer than this process".into());
        }
        metrics.mark_persisted(initial_revision);
        let access = config
            .invite_alpha
            .as_ref()
            .map(|invite| AccessDirectory::new(pool.clone(), invite.quota));
        let sessions = SessionStore::new(
            pool.clone(),
            &config.session_key,
            config.session_ttl,
            &config.public_origin,
            config.invite_alpha.as_ref().map(|invite| invite.quota),
        )?;
        let hepta = HeptaClient::new(
            config.hepta_base.clone(),
            config.assertions.clone(),
            pool.clone(),
            metrics.clone(),
        )?;
        let nakama =
            NakamaArchiveClient::new(config.nakama_base.clone(), config.nakama_http_key.clone())?;
        let cas = CasClient::new(config.cas.clone())?;
        spawn_metrics_persistence(pool.clone(), metrics.clone());
        Ok(Self {
            config: Arc::new(config),
            pool,
            sessions,
            access,
            hepta,
            metrics,
            nakama,
            cas,
        })
    }

    fn identity(&self, subject: &str) -> Option<AlphaIdentity> {
        self.config.identity_for_subject(subject)
    }

    pub(crate) async fn session(
        &self,
        headers: &HeaderMap,
    ) -> Result<AuthenticatedSession, AppError> {
        match self.config.identity_mode {
            IdentityMode::FixedAlpha => {
                self.sessions
                    .authenticate(headers, |subject| self.identity(subject))
                    .await
            }
            IdentityMode::InviteAlpha => {
                let (session_id, subject_id) = self.sessions.authenticate_subject(headers).await?;
                let identity = self
                    .access
                    .as_ref()
                    .ok_or(AppError::Internal)?
                    .identity_for_subject(&subject_id)
                    .await?
                    .ok_or(AppError::Unauthorized)?;
                Ok(AuthenticatedSession {
                    session_id,
                    identity,
                })
            }
        }
    }

    /// Resolves an Agent Bridge owner without accepting a browser session,
    /// login key, cookie, bearer token, or other Agent-side player credential.
    pub(crate) async fn identity_for_agent_bridge(
        &self,
        subject_id: &str,
    ) -> Result<Option<AlphaIdentity>, AppError> {
        match self.config.identity_mode {
            IdentityMode::FixedAlpha => Ok(self.config.identity_for_subject(subject_id)),
            IdentityMode::InviteAlpha => {
                self.access
                    .as_ref()
                    .ok_or(AppError::Internal)?
                    .identity_for_subject(subject_id)
                    .await
            }
        }
    }
}

fn spawn_metrics_persistence(pool: PgPool, metrics: Metrics) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(
            metrics::METRICS_PERSIST_INTERVAL_SECONDS,
        ));
        loop {
            interval.tick().await;
            if !metrics.is_dirty() {
                continue;
            }
            let (snapshot, revision) = match metrics.snapshot_with_revision() {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    tracing::error!(%error, "cannot encode durable BFF metrics snapshot");
                    continue;
                }
            };
            match db::persist_metrics_snapshot(&pool, &snapshot).await {
                Ok(true) => metrics.mark_persisted(revision),
                Ok(false) => {
                    tracing::warn!(
                        revision,
                        "durable BFF metrics snapshot write was superseded by a newer revision"
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "cannot persist durable BFF metrics snapshot; keeping it dirty for retry");
                }
            }
        }
    });
}

pub fn router(state: AppState) -> Router {
    let request_metrics = state.metrics.clone();
    let activation_state = state.clone();
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(operator_metrics))
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
        .route("/api/agent-bindings", get(agent_bindings))
        .route(
            "/api/agent-bridge/pairing-grants",
            get(agent_bridge::pairing_grant_status).post(agent_bridge::create_pairing_grant),
        )
        .route(
            "/api/agent-bridge/pairing-grants/:grant_id/revoke",
            post(agent_bridge::revoke_pairing_grant),
        )
        .route(
            "/api/agent-bridge/pairing-context",
            post(agent_bridge::pairing_context),
        )
        .route("/api/agent-bridge/pair", post(agent_bridge::pair_agent))
        .route(
            "/api/agent-bridge/binding",
            get(agent_bridge::agent_binding),
        )
        .route("/api/agent-bridge/health", post(agent_bridge::agent_health))
        .route(
            "/api/agent-bridge/practice-tasks",
            post(agent_bridge::agent_practice_tasks),
        )
        .route(
            "/api/agent-bridge/practice-claims",
            post(agent_bridge::agent_practice_claim),
        )
        .route(
            "/api/agent-bridge/practice-results",
            post(agent_bridge::agent_practice_result),
        )
        .route("/api/agent-bridge/inbox", post(agent_bridge::agent_inbox))
        .route(
            "/api/agent-bridge/review-objects",
            get(agent_bridge::agent_review_object),
        )
        .route(
            "/api/agent-bridge/review-receipts",
            post(agent_bridge::agent_review_receipt),
        )
        .route(
            "/api/agent-bridge/delivery-drafts",
            post(agent_bridge::agent_delivery_draft),
        )
        .route(
            "/api/agent-bridge/proposals",
            post(agent_bridge::agent_proposal),
        )
        .route("/api/product-events", post(record_browser_product_event))
        .route("/api/papers/:paper_id/timeline", get(paper_timeline))
        .route(
            "/api/papers/:paper_id/outcome",
            post(transition_paper_challenge_outcome),
        )
        .route(
            "/api/papers/:paper_id/artifacts/:digest",
            put(upload_artifact).get(download_artifact),
        )
        .route(
            "/api/review/papers/:paper_id/artifacts/:digest",
            get(download_review_artifact),
        )
        .route(
            "/api/review/papers/:paper_id/receipts/:receipt_id/signing-frame",
            post(crate::review_receipts::signing_frame),
        )
        .route(
            "/api/review/papers/:paper_id/receipts/:receipt_id/confirm",
            post(crate::review_receipts::confirm),
        )
        .route("/league/start", get(league_start))
        .route("/league/onboarding", get(onboarding))
        .route("/league", get(lobby))
        .route("/league/review", get(review_queue_page))
        .route("/league/review/:paper_id", get(review_bundle_page))
        .route("/league/formation/:team_id", get(formation))
        .route("/league/papers/:paper_id", get(paper_room))
        .merge(crate::challenge_materials::router())
        .merge(crate::practice_http::router())
        .merge(crate::quick_raid_http::router())
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            activation_state,
            enforce_invite_activation,
        ))
        .layer(middleware::from_fn_with_state(
            request_metrics,
            metrics::observe_http,
        ))
}

async fn enforce_invite_activation(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if matches!(
        path,
        "/health"
            | "/ready"
            | "/metrics"
            | "/login"
            | "/assets/paper-raid.js"
            | "/assets/paper-raid.css"
    ) {
        return next.run(request).await;
    }
    let active = match &state.config.invite_alpha {
        Some(invite) => db::invite_activation_ready(&state.pool, invite).await,
        None => true,
    };
    if active {
        next.run(request).await
    } else {
        let mut response = (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "invite_activation_unavailable",
                "retryable": false
            })),
        )
            .into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        response
    }
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

async fn operator_metrics(
    State(state): State<AppState>,
    peer: ConnectInfo<SocketAddr>,
) -> Response {
    metrics::loopback_metrics_response(&state.metrics, peer)
}

async fn ready(State(state): State<AppState>) -> Response {
    let (postgres, hepta, nakama, cas, access_status, agent_bridge_status, activation_ready) = tokio::join!(
        db::ready(&state.pool),
        state.hepta.ready(),
        state.nakama.ready(),
        state.cas.ready(),
        async {
            match &state.access {
                Some(access) => Some(access.status().await),
                None => None,
            }
        },
        db::agent_bridge_schema_status(&state.pool),
        async {
            match &state.config.invite_alpha {
                Some(invite) => db::invite_activation_ready(&state.pool, invite).await,
                None => true,
            }
        },
    );
    let trust = state.config.alpha_identity_topology_is_valid()
        && is_root_url(&state.config.hepta_base)
        && is_root_url(&state.config.nakama_base)
        && is_root_url(&state.config.cas.endpoint)
        && matches!(
            state.config.edge_scope,
            EdgeScope::LoopbackProcess | EdgeScope::ContainerLoopbackPublish
        );
    let access_directory_reachable = access_status
        .as_ref()
        .is_none_or(|status| status.directory_reachable);
    let access_directory_within_capacity = access_status
        .as_ref()
        .is_none_or(|status| status.directory_within_capacity);
    let access_audit_append_only = access_status
        .as_ref()
        .is_none_or(|status| status.audit_append_only);
    let access_topology_ready = access_status
        .as_ref()
        .is_none_or(|status| status.topology_ready);
    let ready = postgres
        && hepta
        && nakama
        && cas
        && access_directory_reachable
        && access_directory_within_capacity
        && access_audit_append_only
        && access_topology_ready
        && agent_bridge_status.schema_ready
        && agent_bridge_status.integrity_ok
        && activation_ready
        && trust;
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
            "access_directory_reachable": access_status.as_ref().map(|status| status.directory_reachable),
            "access_directory_within_capacity": access_status.as_ref().map(|status| status.directory_within_capacity),
            "access_audit_append_only": access_status.as_ref().map(|status| status.audit_append_only),
            "access_topology_ready": access_status.as_ref().map(|status| status.topology_ready),
            "access_provisioned_accounts": access_status.as_ref().map(|status| status.provisioned_accounts),
            "access_active_accounts": access_status.as_ref().map(|status| status.active_accounts),
            "access_authors": access_status.as_ref().map(|status| status.authors),
            "access_captains": access_status.as_ref().map(|status| status.captains),
            "access_evidence_authors": access_status.as_ref().map(|status| status.evidence_authors),
            "access_experiment_authors": access_status.as_ref().map(|status| status.experiment_authors),
            "access_evaluators": access_status.as_ref().map(|status| status.evaluators),
            "access_reviewers": access_status.as_ref().map(|status| status.reviewers),
            "access_reproducers": access_status.as_ref().map(|status| status.reproducers),
            "agent_bridge_schema_ready": agent_bridge_status.schema_ready,
            "agent_bridge_integrity_ok": agent_bridge_status.integrity_ok,
            "invite_activation_ready": activation_ready,
            "identity_mode": match state.config.identity_mode {
                IdentityMode::FixedAlpha => "fixed_alpha",
                IdentityMode::InviteAlpha => "invite_alpha",
            },
            "config_trust": trust,
            "finality": "paper_scoped_projection"
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
    let (identity, expected_session_generation) = match state.config.identity_mode {
        IdentityMode::FixedAlpha => (
            state
                .config
                .find_identity(&request.login_key)
                .ok_or(AppError::Unauthorized)?,
            None,
        ),
        IdentityMode::InviteAlpha => {
            let authentication = match state
                .access
                .as_ref()
                .ok_or(AppError::Internal)?
                .authenticate_or_redeem(&request.login_key)
                .await
            {
                Ok(authentication) => authentication,
                Err(error) => {
                    state.metrics.observe_invite(match &error {
                        AppError::Unauthorized | AppError::Forbidden => InviteOutcome::Denied,
                        AppError::RateLimited { .. } => InviteOutcome::RateLimited,
                        _ => InviteOutcome::Error,
                    });
                    return Err(error);
                }
            };
            state
                .metrics
                .observe_invite(if authentication.redeemed_invite {
                    InviteOutcome::Redeemed
                } else {
                    InviteOutcome::Authenticated
                });
            (
                authentication.identity,
                Some(authentication.expected_session_generation),
            )
        }
    };
    let issue = match expected_session_generation {
        Some(generation) => {
            state
                .sessions
                .issue_at_generation(&identity, generation)
                .await?
        }
        None => state.sessions.issue(&identity).await?,
    };
    if let Err(error) = db::record_product_event(
        &state.pool,
        db::ProductEvent {
            event_id: Uuid::new_v4(),
            session_id: None,
            player_id: identity.player_id,
            event_name: "login_succeeded",
            challenge_id: None,
            team_id: None,
            paper_id: None,
            phase: None,
            source: "server_command",
        },
    )
    .await
    {
        tracing::warn!(%error, "could not record login telemetry");
    }
    state.metrics.observe_product_event("login_succeeded");
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
            "scopes": &*session.identity.scopes,
            "author_roles": &*session.identity.author_roles,
            "finality": "paper_scoped_projection"
        }))
        .into_response(),
    ))
}

async fn agent_bindings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    let bindings = state
        .hepta
        .list_current_agent_bindings(&session.identity)
        .await?;
    Ok(private_no_store(Json(bindings).into_response()))
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
            if !identity_requires_agent_binding(&session.identity) {
                return Ok(Redirect::to(identity_home_path(&session.identity)).into_response());
            }
            let bindings = state
                .hepta
                .list_current_agent_bindings(&session.identity)
                .await?;
            if has_active_agent_binding(&bindings, session.identity.player_id)? {
                Ok(Redirect::to(identity_home_path(&session.identity)).into_response())
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
        Ok(_) if !identity_requires_agent_binding(&session.identity) => {
            Ok(Redirect::to(identity_home_path(&session.identity)).into_response())
        }
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
                Ok(true) => Ok(Redirect::to(identity_home_path(&session.identity)).into_response()),
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
    if !identity_scope_allows_command(&session.identity, request.command) {
        return with_rotated_csrf(
            private_no_store(AppError::Forbidden.into_response()),
            next_csrf,
        );
    }
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
    if !identity_scope_allows_command(&session.identity, command.command) {
        return with_rotated_csrf(
            private_no_store(AppError::Forbidden.into_response()),
            next_csrf,
        );
    }
    if !identity_payload_allows_command(&session.identity, &command) {
        return with_rotated_csrf(
            private_no_store(AppError::Forbidden.into_response()),
            next_csrf,
        );
    }
    if command.command == CommandName::QueueMatchmaking {
        let queue_authority = queue_challenge_is_open(&state, &session.identity, &command).await;
        if let Err(error) = queue_authority {
            return with_rotated_csrf(private_no_store(error.into_response()), next_csrf);
        }
    }
    let result = state
        .hepta
        .forward_command(&session.identity, &command)
        .await;
    let response = match result {
        Ok(upstream) => {
            if (200..300).contains(&upstream.status) && !upstream.replayed {
                record_server_command_event(&state, &session, &command, &upstream.body).await;
            }
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

/// Re-read the authoritative challenge catalog immediately before forwarding a
/// queue mutation.  The Lobby's disabled button is only a usability guard: a
/// stale tab or direct request must not be able to enqueue a draft, closed,
/// missing, duplicated, or malformed Challenge through the BFF.
async fn queue_challenge_is_open(
    state: &AppState,
    identity: &AlphaIdentity,
    command: &BrowserCommand,
) -> Result<(), AppError> {
    let challenge_id_text = command
        .payload
        .get("challenge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Invalid("queue challenge_id must be a canonical UUID".into()))?;
    let challenge_id = Uuid::parse_str(challenge_id_text)
        .ok()
        .filter(|value| value.to_string() == challenge_id_text)
        .ok_or_else(|| AppError::Invalid("queue challenge_id must be a canonical UUID".into()))?;
    let catalog = state.hepta.list_public_challenges(identity).await?;
    validate_open_queue_challenge(&catalog, challenge_id)
}

fn validate_open_queue_challenge(catalog: &Value, challenge_id: Uuid) -> Result<(), AppError> {
    let entries = catalog.as_array().ok_or(AppError::Upstream)?;
    let expected = challenge_id.to_string();
    let matches = entries
        .iter()
        .filter(|entry| {
            entry
                .get("challenge_id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == expected.as_str())
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(AppError::Upstream);
    }
    let challenge = matches.into_iter().next().ok_or_else(|| {
        AppError::Conflict("queue challenge is absent from the authoritative catalog".into())
    })?;
    if challenge.get("status").and_then(Value::as_str) != Some("open") {
        return Err(AppError::Conflict(
            "queue challenge is not authoritatively open".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuidedPaperOutcomeRequest {
    outcome: HeptaPaperTerminalOutcome,
    reason_code: String,
}

fn guided_outcome_reason_allowed(outcome: HeptaPaperTerminalOutcome, reason_code: &str) -> bool {
    HeptaPaperTerminalReason::parse_for_outcome(outcome, reason_code).is_some()
}

fn room_actor_is_captain(room: &Value, player_id: Uuid) -> Result<bool, AppError> {
    let members = room
        .get("team")
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let expected = player_id.to_string();
    let matches = members
        .iter()
        .filter(|member| member.get("player_id").and_then(Value::as_str) == Some(expected.as_str()))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::Upstream);
    }
    Ok(matches[0].get("role").and_then(Value::as_str) == Some("captain"))
}

async fn transition_paper_challenge_outcome(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Json(request): Json<GuidedPaperOutcomeRequest>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let response = transition_paper_challenge_outcome_inner(&state, &session, paper_id, request)
        .await
        .unwrap_or_else(|error| error.into_response());
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn transition_paper_challenge_outcome_inner(
    state: &AppState,
    session: &AuthenticatedSession,
    paper_id: Uuid,
    request: GuidedPaperOutcomeRequest,
) -> Result<Response, AppError> {
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    if !guided_outcome_reason_allowed(request.outcome, &request.reason_code) {
        return Err(AppError::Invalid(
            "outcome reason is not valid for the selected terminal outcome".into(),
        ));
    }
    let room = state
        .hepta
        .get_paper_room(&session.identity, paper_id)
        .await?;
    let room = room.value();
    if !room_actor_is_captain(room, session.identity.player_id)? {
        return Err(AppError::Forbidden);
    }
    let paper = room
        .get("paper")
        .filter(|value| value.is_object())
        .ok_or(AppError::Upstream)?;
    if paper
        .get("paper_project_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        != Some(paper_id)
    {
        return Err(AppError::Upstream);
    }
    let current_outcome = paper
        .get("outcome")
        .and_then(Value::as_str)
        .ok_or(AppError::Upstream)?;
    if current_outcome != "in_progress" {
        let same_terminal_fact = current_outcome == request.outcome.as_str()
            && paper.get("outcome_reason").and_then(Value::as_str)
                == Some(request.reason_code.as_str());
        if same_terminal_fact {
            let mut response = Json(paper.clone()).into_response();
            response.headers_mut().insert(
                "x-paper-raid-terminal-recovered",
                HeaderValue::from_static("authoritative-self-read"),
            );
            return Ok(response);
        }
        return Err(AppError::Conflict(
            "paper already has a different immutable challenge outcome".into(),
        ));
    }
    if request.outcome == HeptaPaperTerminalOutcome::Expired {
        let deadline_at = paper
            .get("deadline_at")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Conflict(
                    "expired outcome is unavailable without an authoritative deadline".into(),
                )
            })?;
        let grace_expires_at = paper
            .get("grace_expires_at")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Conflict(
                    "expired outcome is unavailable without an authoritative grace deadline".into(),
                )
            })?;
        let deadline_at = DateTime::parse_from_rfc3339(deadline_at)
            .map_err(|_| AppError::Upstream)?
            .with_timezone(&Utc);
        let grace_expires_at = DateTime::parse_from_rfc3339(grace_expires_at)
            .map_err(|_| AppError::Upstream)?
            .with_timezone(&Utc);
        if grace_expires_at < deadline_at {
            return Err(AppError::Upstream);
        }
        if Utc::now() < grace_expires_at {
            return Err(AppError::Conflict(
                "expired outcome is unavailable until the authoritative grace window elapses"
                    .into(),
            ));
        }
    }
    let expected_version = paper
        .get("version")
        .and_then(Value::as_u64)
        .filter(|version| *version > 0)
        .ok_or(AppError::Upstream)?;
    let command = BrowserCommand {
        command: CommandName::TransitionPaperChallengeOutcome,
        resource_id: Some(paper_id),
        child_id: None,
        session_id: None,
        idempotency_key: Uuid::new_v4(),
        payload: json!({
            "expected_version": expected_version,
            "outcome": request.outcome.as_str(),
            "reason_code": request.reason_code,
        }),
    };
    let upstream = state
        .hepta
        .forward_command(&session.identity, &command)
        .await?;
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
    Ok(response)
}

fn identity_scope_allows_command(identity: &AlphaIdentity, command: CommandName) -> bool {
    let author = || identity.has_scope(AlphaIdentityScope::Author);
    let evaluator = || identity.has_scope(AlphaIdentityScope::Evaluator);
    let reviewer = || identity.has_scope(AlphaIdentityScope::Reviewer);
    let reproducer = || identity.has_scope(AlphaIdentityScope::Reproducer);
    match command {
        CommandName::RotateHumanSigningKey
        | CommandName::RevokeHumanSigningKey
        | CommandName::CreateAgentBinding
        | CommandName::RotateAgentBindingKey => true,
        // Only the dedicated guided endpoint may issue this command. It
        // derives the Paper locator and version from a fresh Hepta room read.
        CommandName::TransitionPaperChallengeOutcome => false,
        CommandName::ClaimReviewAssignment => identity_has_review_scope(identity),
        // The legacy atomic evaluation body bundled two reviewers' signatures.
        // Browser callers must use the immutable draft/quorum protocol instead.
        CommandName::CreatePaperEvaluation => false,
        // Evaluator/reproducer machine results may only enter through the receipt-confirm
        // endpoints.  Those endpoints derive the exact command from a locked, signed receipt.
        CommandName::CreatePaperEvaluationDraft => false,
        CommandName::SubmitEvaluationDraftAttestation => reviewer(),
        CommandName::FinalizePaperEvaluationDraft => evaluator(),
        CommandName::SubmitReproduction => false,
        // The protocol excludes authors, the appellant, evaluator and panel
        // reviewers from resolving an Appeal.  In the seven-identity Alpha
        // topology the independent reproducer is the remaining eligible
        // human authority.
        CommandName::ResolveAppeal => reproducer(),
        CommandName::CreateResearchTeam
        | CommandName::AcceptResearchTeamMembership
        | CommandName::LockResearchTeam
        | CommandName::CreatePaperProject
        | CommandName::TransitionPaperProject
        | CommandName::CreatePaperWorkItem
        | CommandName::TransitionPaperWorkItem
        | CommandName::CreatePaperRevision
        | CommandName::PromotePaperReleaseCandidate
        | CommandName::CreateAuthorshipConsent
        | CommandName::FinalizeJointPaperSubmission
        | CommandName::StartPaperRework
        | CommandName::IssueResearchSessionAuthorizationSet
        | CommandName::ReplaceResearchSessionAuthorizationSet
        | CommandName::CreateNakamaResearchSessionControl
        | CommandName::ResumeNakamaResearchSessionControl
        | CommandName::ReplaceNakamaResearchSessionRosterControl
        | CommandName::CompleteNakamaResearchSessionControl
        | CommandName::QueueMatchmaking
        | CommandName::CancelMatchmakingTicket
        | CommandName::DecideTeamProposal
        | CommandName::MaterializeTeamProposal
        | CommandName::CreateEvidenceCard
        | CommandName::CreateCitationRecord
        | CommandName::CreateExperimentPlan
        | CommandName::CreateRunRecord
        | CommandName::CreateRoleResourceAction
        | CommandName::CreateFigureLineage
        | CommandName::CreateClaimRecord
        | CommandName::AcquireSectionLease
        | CommandName::SubmitAgentProposal
        | CommandName::RecordHumanDecision
        | CommandName::RegisterArtifact
        | CommandName::CreateSectionRevision
        | CommandName::SubmitReview
        | CommandName::MergeSection
        | CommandName::CreateContributionLedger
        | CommandName::SubmitAppeal => author(),
    }
}

fn identity_has_review_scope(identity: &AlphaIdentity) -> bool {
    identity.has_scope(AlphaIdentityScope::Evaluator)
        || identity.has_scope(AlphaIdentityScope::Reviewer)
        || identity.has_scope(AlphaIdentityScope::Reproducer)
}

fn identity_requires_agent_binding(identity: &AlphaIdentity) -> bool {
    identity.has_scope(AlphaIdentityScope::Author)
        || identity.has_scope(AlphaIdentityScope::Evaluator)
        || identity.has_scope(AlphaIdentityScope::Reproducer)
}

fn identity_home_path(identity: &AlphaIdentity) -> &'static str {
    if identity.has_scope(AlphaIdentityScope::Author) {
        "/league"
    } else {
        "/league/review"
    }
}

fn identity_payload_allows_command(identity: &AlphaIdentity, command: &BrowserCommand) -> bool {
    match command.command {
        CommandName::QueueMatchmaking => {
            if !matchmaking_party_payload_is_safe(&command.payload) {
                return false;
            }
            let Some(roles) = command.payload.get("roles").and_then(Value::as_array) else {
                return false;
            };
            if roles.is_empty() || roles.len() > 3 {
                return false;
            }
            let mut seen = HashSet::new();
            roles.iter().all(|value| {
                let Some(role) = value.as_str() else {
                    return false;
                };
                let capability = match role {
                    "captain" => AlphaAuthorRole::Captain,
                    "evidence" => AlphaAuthorRole::Evidence,
                    "experiment" => AlphaAuthorRole::Experiment,
                    _ => return false,
                };
                seen.insert(role) && identity.supports_author_role(capability)
            })
        }
        CommandName::ClaimReviewAssignment => {
            let player_matches = command
                .payload
                .get("player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id);
            let slot_allowed = match command.payload.get("slot").and_then(Value::as_str) {
                Some("evaluator") => identity.has_scope(AlphaIdentityScope::Evaluator),
                Some("reviewer_1" | "reviewer_2") => {
                    identity.has_scope(AlphaIdentityScope::Reviewer)
                }
                Some("reproducer") => identity.has_scope(AlphaIdentityScope::Reproducer),
                _ => false,
            };
            player_matches && slot_allowed
        }
        CommandName::CreatePaperEvaluationDraft => {
            command
                .payload
                .get("evaluator_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        CommandName::SubmitEvaluationDraftAttestation => {
            command
                .payload
                .get("reviewer_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        CommandName::SubmitReproduction => {
            command
                .payload
                .get("reproducer_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        CommandName::SubmitAppeal => {
            command
                .payload
                .get("appellant_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        CommandName::StartPaperRework => {
            command
                .payload
                .get("author_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        CommandName::ResolveAppeal => {
            command
                .payload
                .get("resolver_player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
        }
        _ => true,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserProductEvent {
    event_id: Uuid,
    event_name: String,
    challenge_id: Option<Uuid>,
    team_id: Option<Uuid>,
    paper_id: Option<Uuid>,
    phase: Option<String>,
}

async fn record_browser_product_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<BrowserProductEvent>,
) -> Response {
    let session = match state.session(&headers).await {
        Ok(session) => session,
        Err(error) => return error.into_response(),
    };
    let next_csrf = match state.sessions.consume_csrf(&headers, &session).await {
        Ok(next_csrf) => next_csrf,
        Err(error) => return error.into_response(),
    };
    let allowed = matches!(
        event.event_name.as_str(),
        "abandoned" | "reconnected" | "replay_started" | "continue_opened" | "stale_ui_reload"
    );
    let phase_valid = event.phase.as_ref().is_none_or(|phase| {
        matches!(
            phase.as_str(),
            "forming"
                | "preregistering"
                | "researching"
                | "experimenting"
                | "drafting"
                | "integrity_review"
                | "reproducing"
                | "author_approval"
                | "integrity_hold"
                | "submission_ready"
        )
    });
    let response = if !allowed || !phase_valid {
        AppError::Invalid("unsupported product event".into()).into_response()
    } else if event.event_name == "stale_ui_reload" {
        // This signal is process-local operational telemetry. It deliberately
        // does not expand the durable product-event schema or persist a Paper
        // identifier solely for monitoring.
        state.metrics.observe_stale_ui_reload();
        (StatusCode::NO_CONTENT, Body::empty()).into_response()
    } else {
        match db::record_product_event(
            &state.pool,
            db::ProductEvent {
                event_id: event.event_id,
                session_id: Some(session.session_id),
                player_id: session.identity.player_id,
                event_name: &event.event_name,
                challenge_id: event.challenge_id,
                team_id: event.team_id,
                paper_id: event.paper_id,
                phase: event.phase.as_deref(),
                source: "browser_signal",
            },
        )
        .await
        {
            Ok(()) => (StatusCode::NO_CONTENT, Body::empty()).into_response(),
            Err(error) => AppError::from(error).into_response(),
        }
    };
    with_rotated_csrf(private_no_store(response), next_csrf)
}

async fn record_server_command_event(
    state: &AppState,
    session: &AuthenticatedSession,
    command: &BrowserCommand,
    response_body: &[u8],
) {
    let event_name = match command.command {
        CommandName::QueueMatchmaking => "queue_started",
        CommandName::MaterializeTeamProposal => "team_formed",
        CommandName::CreatePaperProject | CommandName::CreatePaperWorkItem => "first_action",
        CommandName::TransitionPaperProject => "phase_entered",
        CommandName::FinalizeJointPaperSubmission => "raid_completed",
        _ => return,
    };
    let response = serde_json::from_slice::<Value>(response_body).unwrap_or(Value::Null);
    let response_uuid = |field: &str| {
        response
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
    };
    let payload_uuid = |field: &str| {
        command
            .payload
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
    };
    let challenge_id = response_uuid("challenge_id").or_else(|| payload_uuid("challenge_id"));
    let team_id = response_uuid("team_id").or_else(|| payload_uuid("team_id"));
    let paper_id = response_uuid("paper_project_id")
        .or_else(|| payload_uuid("paper_project_id"))
        .or_else(|| {
            matches!(
                command.command,
                CommandName::TransitionPaperProject
                    | CommandName::CreatePaperWorkItem
                    | CommandName::FinalizeJointPaperSubmission
            )
            .then_some(command.resource_id)
            .flatten()
        });
    let phase = response
        .get("phase")
        .and_then(Value::as_str)
        .or_else(|| command.payload.get("next_phase").and_then(Value::as_str));
    if let Err(error) = db::record_product_event(
        &state.pool,
        db::ProductEvent {
            event_id: command.idempotency_key,
            session_id: Some(session.session_id),
            player_id: session.identity.player_id,
            event_name,
            challenge_id,
            team_id,
            paper_id,
            phase,
            source: "server_command",
        },
    )
    .await
    {
        tracing::warn!(%error, event_name, "could not record product telemetry");
    }
    state.metrics.observe_product_event(event_name);
}

async fn lobby(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let (challenges, tickets, proposals, bindings, raid_state) = tokio::join!(
        state.hepta.list_public_challenges(&session.identity),
        state.hepta.list_matchmaking_tickets(&session.identity),
        state.hepta.list_team_proposals(&session.identity),
        state.hepta.list_current_agent_bindings(&session.identity),
        state.hepta.get_player_raid_state(&session.identity),
    );
    state.metrics.observe_match_queue(tickets.as_ref().ok());
    Ok(html::lobby(
        &session.identity,
        read_state(&challenges),
        read_state(&tickets),
        read_state(&proposals),
        read_state(&bindings),
        read_state(&raid_state),
    ))
}

async fn review_queue_page(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !identity_has_review_scope(&session.identity) {
        return Err(AppError::Forbidden);
    }
    let queue = state.hepta.list_review_queue(&session.identity).await;
    Ok(html::review_queue(&session.identity, read_state(&queue)))
}

async fn review_bundle_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !identity_has_review_scope(&session.identity) {
        return Err(AppError::Forbidden);
    }
    let queue = state.hepta.list_review_queue(&session.identity).await?;
    let paper_id_text = paper_id.to_string();
    let assignment = queue
        .as_array()
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("paper_project_id").and_then(Value::as_str) == Some(paper_id_text.as_str())
                    && item
                        .get("my_assignments")
                        .and_then(Value::as_array)
                        .is_some_and(|assignments| !assignments.is_empty())
            })
        })
        .cloned()
        .ok_or(AppError::Forbidden)?;
    let (bundle, review_state) = tokio::join!(
        state
            .hepta
            .get_paper_review_bundle(&session.identity, paper_id),
        state
            .hepta
            .get_paper_review_state(&session.identity, paper_id),
    );
    let bundle = bundle?;
    let bundle = crate::agent_bridge::resolve_frozen_review_bundle(
        &state,
        &bundle,
        session.identity.player_id,
    )
    .await?;
    if !crate::agent_bridge::review_queue_item_matches_resolved_bundle(
        &assignment,
        &bundle,
        session.identity.player_id,
    ) {
        return Err(AppError::Forbidden);
    }
    let receipt_projection =
        crate::review_receipts::pending_projection(&state, &session.identity, paper_id)
            .await
            .unwrap_or_else(|_| {
                json!({
                    "schema": "hepta.paper_raid.review_receipt_projection.v1",
                    "status": "unavailable",
                    "reason_code": "receipt_authority_unavailable",
                    "receipt": Value::Null,
                })
            });
    Ok(html::review_bundle(
        &session.identity,
        &assignment,
        &bundle,
        authenticated_review_value_state(&review_state),
        &receipt_projection,
    ))
}

async fn formation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(resource_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let (proposal, team, acceptances, raid_state, tickets) = tokio::join!(
        state
            .hepta
            .get_team_proposal(&session.identity, resource_id),
        state.hepta.get_team(&session.identity, resource_id),
        state
            .hepta
            .get_team_acceptances(&session.identity, resource_id),
        state.hepta.get_player_raid_state(&session.identity),
        state.hepta.list_matchmaking_tickets(&session.identity),
    );
    Ok(html::formation(
        &session.identity,
        &resource_id.to_string(),
        read_state(&proposal),
        read_state(&team),
        read_state(&acceptances),
        read_state(&raid_state),
        read_state(&tickets),
    ))
}

async fn paper_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let (room, events, review) = tokio::join!(
        state.hepta.get_paper_room(&session.identity, paper_id),
        state
            .hepta
            .list_paper_room_events(&session.identity, paper_id, 0),
        state
            .hepta
            .get_paper_review_state(&session.identity, paper_id),
    );
    let finality = timeline_finality_projection_ref(&review);
    Ok(html::paper_room_with_finality(
        &session.identity,
        &paper_id.to_string(),
        authenticated_room_state(&room),
        read_state(&events),
        authenticated_review_state(&review),
        &finality,
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
    expected_roster_version: Option<u64>,
}

async fn paper_timeline(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(paper_id): Path<Uuid>,
    Query(query): Query<TimelineQuery>,
) -> Result<Response, AppError> {
    validate_timeline_query(&query)?;
    let session = state.session(&headers).await?;
    if !session.identity.has_scope(AlphaIdentityScope::Author) {
        return Err(AppError::Forbidden);
    }
    let (room, events, review) = tokio::join!(
        state.hepta.get_paper_room(&session.identity, paper_id),
        state
            .hepta
            .list_paper_room_events(&session.identity, paper_id, query.after_cursor),
        state
            .hepta
            .get_paper_review_state(&session.identity, paper_id),
    );
    let room = room?;
    let events = events?;
    // Finality is an ancillary projection for the live timeline. A fresh
    // Paper has no release candidate yet, so the review aggregate may be
    // absent/conflicted while authoring is fully available. Keep Paper Room
    // synchronization available, but preserve that epistemic state: unknown,
    // unavailable, and upstream error must never be rewritten as pending.
    let finality = timeline_finality_projection(review);
    state.metrics.observe_finality_availability(&finality);
    let accesses = latest_session_accesses(member_session_accesses(room.value())?, None)?;
    if accesses.len() > MAX_TIMELINE_SESSION_HEADS {
        return Err(AppError::Upstream);
    }
    let live_access = current_live_session_access(&accesses)?;
    let session_heads: Vec<_> = live_access
        .into_iter()
        .map(|access| {
            json!({
                "logical_session_id": access.logical_session_id,
                "roster_version": access.roster_version,
                "nakama_completion_received": access.nakama_completion_received,
            })
        })
        .collect();
    let mut archives = Vec::with_capacity(usize::from(query.logical_session_id.is_some()));
    if let Some(logical_session_id) = query.logical_session_id.as_deref() {
        let Some(access) = live_access else {
            return Err(AppError::Conflict("archive_epoch_changed".into()));
        };
        if access.logical_session_id != logical_session_id
            || query.expected_roster_version != Some(access.roster_version)
        {
            return Err(AppError::Conflict("archive_epoch_changed".into()));
        }
        let archive = state
            .nakama
            .archive(access, paper_id, query.after_sequence)
            .await?;
        archives.push(json!({
            "logical_session_id": access.logical_session_id,
            "roster_version": access.roster_version,
            "nakama_completion_received": access.nakama_completion_received,
            "requested_after_sequence": query.after_sequence,
            "archive": archive,
        }));
    }
    Ok(private_no_store(
        Json(json!({
            "paper_id": paper_id,
            "after_cursor": query.after_cursor,
            "after_sequence": query.after_sequence,
            "paper_room": room.value(),
            "hepta_events": events,
            "research_sessions": session_heads,
            "nakama_archives": archives,
            "finality": finality,
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
                "artifact_sha256": stored.artifact_sha256,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewArtifactQuery {
    assignment_id: Uuid,
    bundle_hash: String,
    object_key: String,
    presentation: Option<ReviewArtifactPresentation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReviewArtifactPresentation {
    Attachment,
    Inline,
}

#[derive(Debug, PartialEq, Eq)]
struct AuthorizedReviewArtifact {
    logical_path: String,
    media_type: String,
    size_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewArtifactAudience {
    BrowserReviewer,
    AgentExecutor,
}

struct ReviewArtifactAuthorization<'a> {
    expected_paper_id: Uuid,
    expected_player_id: Uuid,
    assignment_id: Uuid,
    bundle_hash: &'a str,
    object_key: &'a str,
    digest: &'a str,
    audience: ReviewArtifactAudience,
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
    let media_type = authorized_artifact_media(room.value(), &digest, &expected_uri)?;
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
        HeaderValue::from_static("sandbox; default-src 'none'; frame-ancestors 'none'"),
    );
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

async fn download_review_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((paper_id, digest)): Path<(Uuid, String)>,
    Query(query): Query<ReviewArtifactQuery>,
) -> Result<Response, AppError> {
    let session = state.session(&headers).await?;
    if !identity_has_review_scope(&session.identity) {
        return Err(AppError::Forbidden);
    }
    let hepta_bundle = state
        .hepta
        .get_paper_review_bundle(&session.identity, paper_id)
        .await?;
    let bundle = crate::agent_bridge::resolve_frozen_review_bundle(
        &state,
        &hepta_bundle,
        session.identity.player_id,
    )
    .await?;
    let artifact = authorized_review_artifact(
        &bundle,
        ReviewArtifactAuthorization {
            expected_paper_id: paper_id,
            expected_player_id: session.identity.player_id,
            assignment_id: query.assignment_id,
            bundle_hash: &query.bundle_hash,
            object_key: &query.object_key,
            digest: &digest,
            audience: ReviewArtifactAudience::BrowserReviewer,
        },
    )?;
    state.cas.validate_media_type(&artifact.media_type)?;
    let bytes = state.cas.get(&digest, &artifact.media_type).await?;
    if bytes.len() as u64 != artifact.size_bytes {
        return Err(AppError::Upstream);
    }
    review_artifact_browser_response(
        bytes,
        &artifact.media_type,
        &artifact.logical_path,
        query
            .presentation
            .unwrap_or(ReviewArtifactPresentation::Attachment),
    )
}

fn authorized_review_artifact(
    review_bundle: &Value,
    authorization: ReviewArtifactAuthorization<'_>,
) -> Result<AuthorizedReviewArtifact, AppError> {
    crate::cas::raw_sha256(authorization.digest)?;
    crate::cas::raw_sha256(authorization.bundle_hash)?;
    let descriptor = verified_review_descriptor_scope(
        review_bundle,
        authorization.expected_paper_id,
        authorization.expected_player_id,
    )?;
    if descriptor.assignment_id != authorization.assignment_id
        || descriptor.bundle_hash != authorization.bundle_hash
    {
        return Err(AppError::Forbidden);
    }
    let objects = match authorization.audience {
        ReviewArtifactAudience::BrowserReviewer => &descriptor.authority.artifact_objects,
        ReviewArtifactAudience::AgentExecutor => &descriptor.objects,
    };
    let matches = objects
        .iter()
        .filter(|object| {
            object.object_key == authorization.object_key
                && object.digest == authorization.digest
                && object.download_path
                    == hepta_paper_raid_contracts::REVIEW_OBJECT_DOWNLOAD_PATH_V1
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(AppError::NotFound);
    }
    let object = matches[0];
    if !review_artifact_logical_path_is_safe(&object.logical_path)
        || !review_artifact_object_key_is_safe(&object.object_key)
        || !(1..=16 * 1024 * 1024).contains(&object.size_bytes)
        || (authorization.audience == ReviewArtifactAudience::AgentExecutor
            && !matches!(
                object.role.as_str(),
                "candidate" | "dataset" | "evaluator_support" | "frozen_evaluator"
            ))
    {
        return Err(AppError::Upstream);
    }
    crate::cas::validate_media_type(&object.media_type).map_err(|_| AppError::Upstream)?;
    Ok(AuthorizedReviewArtifact {
        logical_path: object.logical_path.clone(),
        media_type: object.media_type.clone(),
        size_bytes: object.size_bytes,
    })
}

fn review_assignment_matches_descriptor(
    assignment: &Value,
    descriptor: &FrozenReviewBundleV1,
    expected_player_id: Uuid,
) -> bool {
    let assignment_id = descriptor.assignment_id.to_string();
    let paper_id = descriptor.paper_project_id.to_string();
    let submission_id = descriptor.submission_id.to_string();
    let player_id = expected_player_id.to_string();
    assignment.get("assignment_id").and_then(Value::as_str) == Some(assignment_id.as_str())
        && assignment.get("paper_project_id").and_then(Value::as_str) == Some(paper_id.as_str())
        && assignment.get("submission_id").and_then(Value::as_str) == Some(submission_id.as_str())
        && assignment.get("player_id").and_then(Value::as_str) == Some(player_id.as_str())
        && assignment.get("review_round").and_then(Value::as_u64) == Some(descriptor.review_round)
        && assignment.get("slot").and_then(Value::as_str) == Some(descriptor.slot.as_str())
        && assignment.get("version").and_then(Value::as_u64) == Some(descriptor.assignment_version)
        && assignment.get("expires_at").and_then(Value::as_str)
            == Some(descriptor.expires_at.as_str())
        && matches!(
            assignment.get("status").and_then(Value::as_str),
            Some("claimed" | "pinned")
        )
}

fn verified_review_descriptor_scope(
    review_bundle: &Value,
    expected_paper_id: Uuid,
    expected_player_id: Uuid,
) -> Result<FrozenReviewBundleV1, AppError> {
    let descriptor: FrozenReviewBundleV1 = serde_json::from_value(
        review_bundle
            .get("resolved_frozen_review_bundle")
            .cloned()
            .ok_or(AppError::Upstream)?,
    )
    .map_err(|_| AppError::Upstream)?;
    verify_frozen_review_bundle(&descriptor).map_err(|_| AppError::Upstream)?;
    let paper_id_text = expected_paper_id.to_string();
    let submission_id_text = descriptor.submission_id.to_string();
    if !crate::agent_bridge::review_outer_bundle_matches_authority(
        review_bundle,
        &descriptor.authority,
    ) || descriptor.paper_project_id != expected_paper_id
        || review_bundle
            .get("paper_project_id")
            .and_then(Value::as_str)
            != Some(paper_id_text.as_str())
        || review_bundle.get("submission_id").and_then(Value::as_str)
            != Some(submission_id_text.as_str())
        || review_bundle
            .get("release_candidate_hash")
            .and_then(Value::as_str)
            != Some(descriptor.release_candidate_hash.as_str())
        || review_bundle
            .get("paper_bundle_hash")
            .and_then(Value::as_str)
            != Some(descriptor.paper_bundle_hash.as_str())
    {
        return Err(AppError::Forbidden);
    }
    let assignments = review_bundle
        .get("my_assignments")
        .and_then(Value::as_array)
        .ok_or(AppError::Upstream)?;
    let assignment = assignments.first().ok_or(AppError::Forbidden)?;
    if assignments.len() != 1
        || !review_assignment_matches_descriptor(assignment, &descriptor, expected_player_id)
    {
        return Err(AppError::Forbidden);
    }
    Ok(descriptor)
}

fn review_artifact_logical_path_is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains('\0')
        && !value.bytes().any(|byte| byte.is_ascii_control())
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn review_artifact_object_key_is_safe(value: &str) -> bool {
    let mut bytes = value.bytes();
    value.len() <= 128
        && bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

pub(crate) fn authorized_review_artifact_media(
    review_bundle: &Value,
    expected_paper_id: Uuid,
    expected_player_id: Uuid,
    assignment_id: Uuid,
    bundle_hash: &str,
    object_key: &str,
    digest: &str,
) -> Result<(String, u64), AppError> {
    authorized_review_artifact(
        review_bundle,
        ReviewArtifactAuthorization {
            expected_paper_id,
            expected_player_id,
            assignment_id,
            bundle_hash,
            object_key,
            digest,
            audience: ReviewArtifactAudience::AgentExecutor,
        },
    )
    .map(|artifact| (artifact.media_type, artifact.size_bytes))
}

fn review_artifact_download_filename(logical_path: &str) -> String {
    let filename = logical_path.rsplit('/').next().unwrap_or_default();
    let sanitized = filename
        .chars()
        .take(128)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() || matches!(sanitized.as_str(), "." | "..") {
        "paper-raid-frozen-review-object".to_string()
    } else {
        sanitized
    }
}

fn review_artifact_browser_response(
    bytes: Vec<u8>,
    media_type: &str,
    logical_path: &str,
    presentation: ReviewArtifactPresentation,
) -> Result<Response, AppError> {
    let filename = review_artifact_download_filename(logical_path);
    let disposition = HeaderValue::from_str(&format!(
        "{}; filename=\"{}\"",
        match presentation {
            ReviewArtifactPresentation::Attachment => "attachment",
            ReviewArtifactPresentation::Inline => "inline",
        },
        filename,
    ))
    .map_err(|_| AppError::Upstream)?;
    review_artifact_response_with_disposition(bytes, media_type, disposition)
}

pub(crate) fn review_artifact_response(
    bytes: Vec<u8>,
    media_type: &str,
) -> Result<Response, AppError> {
    review_artifact_response_with_disposition(
        bytes,
        media_type,
        HeaderValue::from_static("attachment; filename=paper-raid-frozen-review-object"),
    )
}

fn review_artifact_response_with_disposition(
    bytes: Vec<u8>,
    media_type: &str,
    disposition: HeaderValue,
) -> Result<Response, AppError> {
    let content_type = HeaderValue::from_str(media_type).map_err(|_| AppError::Upstream)?;
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
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'; frame-ancestors 'none'"),
    );
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    response.headers_mut().insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    Ok(response)
}

fn latest_session_accesses(
    accesses: Vec<MemberSessionAccess>,
    logical_session_id: Option<&str>,
) -> Result<Vec<MemberSessionAccess>, AppError> {
    if logical_session_id.is_some_and(|value| !valid_logical_session_id(value)) {
        return Err(AppError::Invalid("logical_session_id is invalid".into()));
    }
    let mut latest: BTreeMap<String, MemberSessionAccess> = BTreeMap::new();
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

fn current_live_session_access(
    accesses: &[MemberSessionAccess],
) -> Result<Option<&MemberSessionAccess>, AppError> {
    let mut live = accesses.iter().filter(|access| {
        matches!(access.status.as_str(), "issued" | "consumed")
            && !access.nakama_completion_received
    });
    let current = live.next();
    if live.next().is_some() {
        return Err(AppError::Upstream);
    }
    Ok(current)
}

const MAX_TIMELINE_SESSION_HEADS: usize = 64;
const JSON_SAFE_U64_MAX: u64 = 9_007_199_254_740_991;

fn valid_logical_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn validate_timeline_query(query: &TimelineQuery) -> Result<(), AppError> {
    if query.after_cursor > JSON_SAFE_U64_MAX
        || query.after_sequence > JSON_SAFE_U64_MAX
        || query
            .expected_roster_version
            .is_some_and(|version| version == 0 || version > JSON_SAFE_U64_MAX)
        || query
            .logical_session_id
            .as_deref()
            .is_some_and(|value| !valid_logical_session_id(value))
        || query.logical_session_id.is_some() != query.expected_roster_version.is_some()
        || (query.logical_session_id.is_none() && query.after_sequence != 0)
    {
        return Err(AppError::Invalid("timeline cursor is invalid".into()));
    }
    Ok(())
}

fn authorized_artifact_media(
    room: &Value,
    digest: &str,
    expected_uri: &str,
) -> Result<String, AppError> {
    let artifact_sha256 = crate::cas::raw_sha256(digest)?;
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
            .filter(|object| object.get("sha256").and_then(Value::as_str) == Some(artifact_sha256))
        {
            let logical_path = object
                .get("logical_path")
                .and_then(Value::as_str)
                .ok_or(AppError::Upstream)?;
            let media_type = object
                .get("media_type")
                .and_then(Value::as_str)
                .ok_or(AppError::Upstream)?;
            crate::cas::validate_media_type(media_type).map_err(|_| AppError::Upstream)?;
            let matches: Vec<_> = locations
                .iter()
                .filter(|location| {
                    location.get("logical_path").and_then(Value::as_str) == Some(logical_path)
                        && location.get("sha256").and_then(Value::as_str) == Some(artifact_sha256)
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
    Ok(bytes)
}

fn with_cookie(mut response: Response, cookie: HeaderValue) -> Response {
    response.headers_mut().append(header::SET_COOKIE, cookie);
    response
}

pub(crate) fn with_rotated_csrf(mut response: Response, csrf: String) -> Response {
    if let Ok(value) = HeaderValue::from_str(&csrf) {
        response.headers_mut().insert(CSRF_HEADER, value);
    }
    response
}

pub(crate) fn private_no_store(mut response: Response) -> Response {
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

fn authenticated_room_state(
    result: &Result<AuthenticatedPaperRoom, AppError>,
) -> html::ReadState<'_, AuthenticatedPaperRoom> {
    match result {
        Ok(value) => html::ReadState::Available(value),
        Err(AppError::NotFound) => html::ReadState::NotFound,
        Err(_) => html::ReadState::Unavailable,
    }
}

fn authenticated_review_state(
    result: &Result<AuthenticatedPaperReviewState, AppError>,
) -> html::ReadState<'_, AuthenticatedPaperReviewState> {
    match result {
        Ok(value) => html::ReadState::Available(value),
        Err(AppError::NotFound) => html::ReadState::NotFound,
        Err(_) => html::ReadState::Unavailable,
    }
}

fn authenticated_review_value_state(
    result: &Result<AuthenticatedPaperReviewState, AppError>,
) -> html::ReadState<'_> {
    match result {
        Ok(value) => html::ReadState::Available(value.value()),
        Err(AppError::NotFound) => html::ReadState::NotFound,
        Err(_) => html::ReadState::Unavailable,
    }
}

const FINALITY_AVAILABILITY_SCHEMA_V1: &str = "hepta.paper_raid.bff_finality_availability.v1";

fn non_authoritative_finality_availability(status: &str, reason_code: &str) -> Value {
    json!({
        "schema": FINALITY_AVAILABILITY_SCHEMA_V1,
        "status": status,
        "authoritative": false,
        "reason_code": reason_code,
        "ranking_eligible": false,
        "reward_eligible": false,
        "score_eligible": false,
        "economic_eligible": false,
        "verified_at": null,
    })
}

fn timeline_finality_projection(review: Result<AuthenticatedPaperReviewState, AppError>) -> Value {
    timeline_finality_projection_ref(&review)
}

fn timeline_finality_projection_ref(
    review: &Result<AuthenticatedPaperReviewState, AppError>,
) -> Value {
    match review {
        Ok(value) => value.finality().clone(),
        Err(AppError::NotFound) => non_authoritative_finality_availability(
            "unknown_finality",
            "review_aggregate_not_found",
        ),
        Err(AppError::Conflict(_)) => non_authoritative_finality_availability(
            "unavailable_finality",
            "review_projection_conflict",
        ),
        Err(error) => {
            tracing::warn!(%error, "Paper finality projection unavailable during timeline read");
            non_authoritative_finality_availability("error_finality", "review_projection_error")
        }
    }
}

fn has_active_agent_binding(value: &Value, player_id: Uuid) -> Result<bool, AppError> {
    let bindings = value.as_array().ok_or(AppError::Upstream)?;
    let mut active_count = 0_u32;
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
            "active" => active_count += 1,
            "revoked" => {}
            _ => return Err(AppError::Upstream),
        }
    }
    Ok(active_count == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_challenge_catalog_is_open_only_and_fail_closed() {
        let challenge_id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let open = json!([{
            "challenge_id": challenge_id,
            "status": "open"
        }]);
        validate_open_queue_challenge(&open, challenge_id).expect("open challenge queues");

        for status in ["closed", "draft", "unknown"] {
            let catalog = json!([{
                "challenge_id": challenge_id,
                "status": status
            }]);
            assert!(matches!(
                validate_open_queue_challenge(&catalog, challenge_id),
                Err(AppError::Conflict(_))
            ));
        }
        assert!(matches!(
            validate_open_queue_challenge(&json!([]), challenge_id),
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            validate_open_queue_challenge(&json!({"challenges": []}), challenge_id),
            Err(AppError::Upstream)
        ));

        let duplicate = json!([
            {"challenge_id": challenge_id, "status": "open"},
            {"challenge_id": challenge_id, "status": "closed"}
        ]);
        assert!(matches!(
            validate_open_queue_challenge(&duplicate, challenge_id),
            Err(AppError::Upstream)
        ));

        let malformed_id = json!([{"challenge_id": "not-a-uuid", "status": "open"}]);
        assert!(matches!(
            validate_open_queue_challenge(&malformed_id, challenge_id),
            Err(AppError::Conflict(_))
        ));
    }

    #[test]
    fn identity_scopes_separate_author_evaluator_and_reproducer_commands() {
        let author = AlphaIdentity::test_identity("author", Uuid::new_v4(), Uuid::new_v4());
        assert!(identity_scope_allows_command(
            &author,
            CommandName::QueueMatchmaking
        ));
        assert!(identity_scope_allows_command(
            &author,
            CommandName::CreateRoleResourceAction
        ));
        assert!(identity_scope_allows_command(
            &author,
            CommandName::StartPaperRework
        ));
        assert!(!identity_scope_allows_command(
            &author,
            CommandName::TransitionPaperChallengeOutcome
        ));
        assert!(!identity_scope_allows_command(
            &author,
            CommandName::CreatePaperEvaluation
        ));

        let mut evaluator =
            AlphaIdentity::test_identity("evaluator", Uuid::new_v4(), Uuid::new_v4());
        evaluator.scopes = Arc::from([AlphaIdentityScope::Evaluator]);
        evaluator.author_roles = Arc::from([]);
        assert_eq!(identity_home_path(&evaluator), "/league/review");
        assert!(identity_requires_agent_binding(&evaluator));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::CreatePaperEvaluation
        ));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::CreatePaperEvaluationDraft
        ));
        assert!(identity_scope_allows_command(
            &evaluator,
            CommandName::FinalizePaperEvaluationDraft
        ));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::SubmitEvaluationDraftAttestation
        ));
        assert!(identity_scope_allows_command(
            &evaluator,
            CommandName::ClaimReviewAssignment
        ));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::QueueMatchmaking
        ));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::CreateRoleResourceAction
        ));
        assert!(!identity_scope_allows_command(
            &evaluator,
            CommandName::StartPaperRework
        ));

        let mut reproducer =
            AlphaIdentity::test_identity("reproducer", Uuid::new_v4(), Uuid::new_v4());
        reproducer.scopes = Arc::from([AlphaIdentityScope::Reproducer]);
        reproducer.author_roles = Arc::from([]);
        assert_eq!(identity_home_path(&reproducer), "/league/review");
        assert!(identity_requires_agent_binding(&reproducer));
        assert!(!identity_scope_allows_command(
            &reproducer,
            CommandName::SubmitReproduction
        ));
        assert!(identity_scope_allows_command(
            &reproducer,
            CommandName::ResolveAppeal
        ));
        assert!(!identity_scope_allows_command(
            &reproducer,
            CommandName::CreatePaperEvaluation
        ));
        assert!(!identity_scope_allows_command(
            &reproducer,
            CommandName::CreateRoleResourceAction
        ));

        let mut mixed_author = evaluator.clone();
        mixed_author.scopes =
            Arc::from([AlphaIdentityScope::Author, AlphaIdentityScope::Evaluator]);
        assert!(!identity_scope_allows_command(
            &mixed_author,
            CommandName::CreatePaperEvaluationDraft
        ));
        assert!(identity_has_review_scope(&mixed_author));
    }

    #[test]
    fn guided_terminal_outcome_reasons_are_typed_and_outcome_specific() {
        assert!(guided_outcome_reason_allowed(
            HeptaPaperTerminalOutcome::Failed,
            "preregistered_result_failed"
        ));
        assert!(guided_outcome_reason_allowed(
            HeptaPaperTerminalOutcome::Abandoned,
            "team_withdrawal"
        ));
        assert!(guided_outcome_reason_allowed(
            HeptaPaperTerminalOutcome::Expired,
            "challenge_grace_deadline_elapsed"
        ));
        assert!(!guided_outcome_reason_allowed(
            HeptaPaperTerminalOutcome::Expired,
            "team_withdrawal"
        ));
        assert!(!guided_outcome_reason_allowed(
            HeptaPaperTerminalOutcome::Failed,
            "invented"
        ));
    }

    #[test]
    fn matchmaking_payload_cannot_exceed_identity_role_capabilities() {
        let mut evidence = AlphaIdentity::test_identity("evidence", Uuid::new_v4(), Uuid::new_v4());
        evidence.author_roles = Arc::from([AlphaAuthorRole::Evidence]);
        let command = |roles: Value| BrowserCommand {
            command: CommandName::QueueMatchmaking,
            resource_id: None,
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: json!({"roles": roles}),
        };
        assert!(identity_payload_allows_command(
            &evidence,
            &command(json!(["evidence"]))
        ));
        assert!(!identity_payload_allows_command(
            &evidence,
            &command(json!(["captain"]))
        ));
        assert!(!identity_payload_allows_command(
            &evidence,
            &command(json!(["evidence", "evidence"]))
        ));
        assert!(!identity_payload_allows_command(
            &evidence,
            &command(json!(["invented"]))
        ));
    }

    #[test]
    fn matchmaking_payload_never_forwards_a_raw_or_noncanonical_party_code() {
        let mut captain = AlphaIdentity::test_identity("captain", Uuid::new_v4(), Uuid::new_v4());
        captain.author_roles = Arc::from([AlphaAuthorRole::Captain]);
        let command = |payload: Value| BrowserCommand {
            command: CommandName::QueueMatchmaking,
            resource_id: None,
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload,
        };
        assert!(identity_payload_allows_command(
            &captain,
            &command(json!({
                "roles":["captain"],
                "party_code_hash":format!("sha256:{}", "a".repeat(64)),
            }))
        ));
        for payload in [
            json!({"roles":["captain"],"party_code":"PR1-raw"}),
            json!({"roles":["captain"],"raw_party_code":"PR1-raw"}),
            json!({"roles":["captain"],"party_code_hash":format!("sha256:{}", "A".repeat(64))}),
        ] {
            assert!(!identity_payload_allows_command(
                &captain,
                &command(payload)
            ));
        }
    }

    #[test]
    fn review_claim_payload_is_bound_to_identity_and_exact_slot_scope() {
        let mut reviewer = AlphaIdentity::test_identity("reviewer", Uuid::new_v4(), Uuid::new_v4());
        reviewer.scopes = Arc::from([AlphaIdentityScope::Reviewer]);
        reviewer.author_roles = Arc::from([]);
        let command = |player_id: Uuid, slot: &str| BrowserCommand {
            command: CommandName::ClaimReviewAssignment,
            resource_id: Some(Uuid::new_v4()),
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: json!({
                "assignment_id": Uuid::new_v4(),
                "player_id": player_id,
                "review_round": 1,
                "slot": slot,
            }),
        };
        assert!(identity_payload_allows_command(
            &reviewer,
            &command(reviewer.player_id, "reviewer_1")
        ));
        assert!(!identity_payload_allows_command(
            &reviewer,
            &command(Uuid::new_v4(), "reviewer_1")
        ));
        assert!(!identity_payload_allows_command(
            &reviewer,
            &command(reviewer.player_id, "evaluator")
        ));
        assert!(!identity_payload_allows_command(
            &reviewer,
            &command(reviewer.player_id, "invented")
        ));
    }

    #[test]
    fn timeline_never_rewrites_unknown_or_unavailable_finality_as_pending() {
        let unavailable = timeline_finality_projection(Err(AppError::Conflict(
            "release_candidate_required".into(),
        )));
        assert_eq!(unavailable["status"], "unavailable_finality");
        assert_eq!(
            unavailable["schema"],
            "hepta.paper_raid.bff_finality_availability.v1"
        );
        assert_eq!(unavailable["authoritative"], false);
        assert_eq!(unavailable["reason_code"], "review_projection_conflict");
        assert_eq!(unavailable["ranking_eligible"], false);
        assert_eq!(unavailable["reward_eligible"], false);
        assert_eq!(unavailable["score_eligible"], false);
        assert_eq!(unavailable["economic_eligible"], false);

        let missing = timeline_finality_projection(Err(AppError::NotFound));
        assert_eq!(missing["status"], "unknown_finality");
        assert_eq!(missing["reason_code"], "review_aggregate_not_found");

        let malformed = timeline_finality_projection(
            AuthenticatedPaperReviewState::test_only_seal(Uuid::new_v4(), json!({})),
        );
        assert_eq!(malformed["status"], "error_finality");
        assert_eq!(malformed["reason_code"], "review_projection_error");

        let failed = timeline_finality_projection(Err(AppError::Upstream));
        assert_eq!(failed["status"], "error_finality");
        assert_eq!(failed["reason_code"], "review_projection_error");

        let paper_id = Uuid::new_v4();
        let evaluation_id = Uuid::new_v4();
        let reproduction_id = Uuid::new_v4();
        let verified = AuthenticatedPaperReviewState::test_only_seal(
            paper_id,
            json!({
                "finality": {
                    "schema": "hepta.paper_raid.consumer_finality.v2",
                    "status": "verified_finality",
                    "effective_evaluation_id": evaluation_id,
                    "effective_reproduction_id": reproduction_id,
                    "effective_appeal_resolution_id": null,
                    "ranking_eligible": false,
                    "reward_eligible": false,
                    "score_eligible": false,
                    "economic_eligible": false,
                    "verified_at": "2026-08-10T00:00:00Z"
                },
                "assignments": [],
                "evaluation_drafts": [],
                "contribution_ledgers": [],
                "evaluations": [],
                "reproductions": [],
                "appeals": [],
                "resolutions": [],
                "raid_scores": []
            }),
        )
        .expect("strict authenticated review fixture");
        assert_eq!(
            timeline_finality_projection(Ok(verified))["status"],
            "verified_finality"
        );
    }

    #[test]
    fn timeline_rejects_malformed_or_eligible_authoritative_finality() {
        let paper_id = Uuid::new_v4();
        let envelope = |finality: Value| {
            json!({
                "finality": finality,
                "assignments": [],
                "evaluation_drafts": [],
                "contribution_ledgers": [],
                "evaluations": [],
                "reproductions": [],
                "appeals": [],
                "resolutions": [],
                "raid_scores": []
            })
        };
        let malformed_schema = json!({
            "schema": "hepta.paper_raid.bff_finality_availability.v1",
            "status": "verified_finality",
            "effective_evaluation_id": Uuid::new_v4(),
            "effective_reproduction_id": Uuid::new_v4(),
            "effective_appeal_resolution_id": null,
            "ranking_eligible": false,
            "reward_eligible": false,
            "score_eligible": false,
            "economic_eligible": false,
            "verified_at": "2026-08-10T00:00:00Z"
        });
        let projection = timeline_finality_projection(
            AuthenticatedPaperReviewState::test_only_seal(paper_id, envelope(malformed_schema)),
        );
        assert_eq!(projection["status"], "error_finality");
        assert_eq!(projection["reason_code"], "review_projection_error");

        let unexpectedly_eligible = json!({
            "schema": "hepta.paper_raid.consumer_finality.v2",
            "status": "verified_finality",
            "effective_evaluation_id": Uuid::new_v4(),
            "effective_reproduction_id": Uuid::new_v4(),
            "effective_appeal_resolution_id": null,
            "ranking_eligible": true,
            "reward_eligible": false,
            "score_eligible": false,
            "economic_eligible": false,
            "verified_at": "2026-08-10T00:00:00Z"
        });
        let projection =
            timeline_finality_projection(AuthenticatedPaperReviewState::test_only_seal(
                paper_id,
                envelope(unexpectedly_eligible),
            ));
        assert_eq!(projection["status"], "error_finality");
        assert_eq!(projection["reason_code"], "review_projection_error");

        let pending_with_timestamp = json!({
            "schema": "hepta.paper_raid.consumer_finality.v2",
            "status": "pending_finality",
            "effective_evaluation_id": null,
            "effective_reproduction_id": null,
            "effective_appeal_resolution_id": null,
            "ranking_eligible": false,
            "reward_eligible": false,
            "score_eligible": false,
            "economic_eligible": false,
            "verified_at": "2026-08-10T00:00:00Z"
        });
        assert_eq!(
            timeline_finality_projection(AuthenticatedPaperReviewState::test_only_seal(
                paper_id,
                envelope(pending_with_timestamp),
            ))["reason_code"],
            "review_projection_error"
        );

        let extra_field = json!({
            "schema": "hepta.paper_raid.consumer_finality.v2",
            "status": "pending_finality",
            "effective_evaluation_id": null,
            "effective_reproduction_id": null,
            "effective_appeal_resolution_id": null,
            "ranking_eligible": false,
            "reward_eligible": false,
            "score_eligible": false,
            "economic_eligible": false,
            "verified_at": null,
            "authoritative": true
        });
        assert_eq!(
            timeline_finality_projection(AuthenticatedPaperReviewState::test_only_seal(
                paper_id,
                envelope(extra_field),
            ))["reason_code"],
            "review_projection_error"
        );
    }

    #[test]
    fn rendered_review_state_rejects_invalid_finality_before_html() {
        let paper_id = Uuid::new_v4();
        let review = json!({
            "finality": {
                "schema": "hepta.paper_raid.consumer_finality.v2",
                "status": "verified_finality",
                "effective_evaluation_id": Uuid::new_v4(),
                "effective_reproduction_id": Uuid::new_v4(),
                "effective_appeal_resolution_id": null,
                "ranking_eligible": false,
                "reward_eligible": false,
                "score_eligible": false,
                "economic_eligible": true,
                "verified_at": "2026-08-10T00:00:00Z"
            },
            "assignments": [],
            "evaluation_drafts": [],
            "contribution_ledgers": [],
            "evaluations": [],
            "reproductions": [],
            "appeals": [],
            "resolutions": [],
            "raid_scores": []
        });
        assert!(matches!(
            AuthenticatedPaperReviewState::test_only_seal(paper_id, review),
            Err(AppError::Upstream)
        ));
    }

    #[test]
    fn timeline_selects_only_latest_player_scoped_roster() {
        let logical_session_id = "paper.raid:alpha".to_string();
        let accesses = vec![
            MemberSessionAccess {
                logical_session_id: logical_session_id.clone(),
                authorization_id: Uuid::new_v4(),
                roster_version: 1,
                status: "superseded".into(),
                nakama_completion_received: false,
            },
            MemberSessionAccess {
                logical_session_id: logical_session_id.clone(),
                authorization_id: Uuid::new_v4(),
                roster_version: 2,
                status: "completed".into(),
                nakama_completion_received: true,
            },
            MemberSessionAccess {
                logical_session_id: "paper.raid:other".into(),
                authorization_id: Uuid::new_v4(),
                roster_version: 1,
                status: "issued".into(),
                nakama_completion_received: false,
            },
        ];
        let selected = latest_session_accesses(accesses.clone(), Some(&logical_session_id))
            .expect("latest scoped access");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].roster_version, 2);
        assert!(selected[0].nakama_completion_received);
        let latest = latest_session_accesses(accesses.clone(), None).expect("latest heads");
        assert_eq!(
            current_live_session_access(&latest)
                .expect("one live head")
                .expect("live head")
                .logical_session_id,
            "paper.raid:other"
        );
        let mut ambiguous = latest.clone();
        ambiguous.push(MemberSessionAccess {
            logical_session_id: "paper.raid:third".into(),
            authorization_id: Uuid::new_v4(),
            roster_version: 1,
            status: "consumed".into(),
            nakama_completion_received: false,
        });
        assert!(matches!(
            current_live_session_access(&ambiguous),
            Err(AppError::Upstream)
        ));
        assert!(matches!(
            latest_session_accesses(accesses, Some("not-visible")),
            Err(AppError::NotFound)
        ));
    }

    #[test]
    fn timeline_requires_an_exact_nakama_session_epoch() {
        for valid in [
            TimelineQuery {
                after_cursor: 7,
                after_sequence: 0,
                logical_session_id: None,
                expected_roster_version: None,
            },
            TimelineQuery {
                after_cursor: 7,
                after_sequence: 19,
                logical_session_id: Some("paper.raid:alpha".into()),
                expected_roster_version: Some(2),
            },
        ] {
            validate_timeline_query(&valid).expect("valid timeline cursor");
        }

        for invalid in [
            TimelineQuery {
                after_cursor: 0,
                after_sequence: 1,
                logical_session_id: None,
                expected_roster_version: None,
            },
            TimelineQuery {
                after_cursor: 0,
                after_sequence: 1,
                logical_session_id: Some("paper.raid:alpha".into()),
                expected_roster_version: None,
            },
            TimelineQuery {
                after_cursor: 0,
                after_sequence: 1,
                logical_session_id: Some("bad/session".into()),
                expected_roster_version: Some(1),
            },
            TimelineQuery {
                after_cursor: 0,
                after_sequence: 1,
                logical_session_id: Some("paper.raid:alpha".into()),
                expected_roster_version: Some(0),
            },
        ] {
            assert!(matches!(
                validate_timeline_query(&invalid),
                Err(AppError::Invalid(_))
            ));
        }
    }

    #[test]
    fn artifact_download_requires_exact_hepta_manifest_uri_acl_and_media() {
        let artifact_sha256 = "ab".repeat(32);
        let digest = format!("sha256:{artifact_sha256}");
        let expected_uri = format!("cas://sha256/{artifact_sha256}");
        let room = json!({
            "artifact_manifests": [{
                "objects": [{
                    "canonical_json": false,
                    "dependencies": [],
                    "logical_path": "paper/main.md",
                    "media_type": "text/markdown; charset=utf-8",
                    "role": "paper_source",
                    "sha256": artifact_sha256,
                    "size": 12
                }],
                "storage_locations": [{
                    "logical_path": "paper/main.md",
                    "sha256": artifact_sha256,
                    "uri": expected_uri,
                    "acl": "team"
                }]
            }]
        });
        assert_eq!(
            authorized_artifact_media(&room, &digest, &expected_uri).expect("authorized"),
            "text/markdown; charset=utf-8"
        );
        let mut tampered = room.clone();
        tampered["artifact_manifests"][0]["storage_locations"][0]["uri"] =
            Value::String(format!("{}://attacker/objects/sha256/deadbeef", "s3"));
        assert!(matches!(
            authorized_artifact_media(&tampered, &digest, &expected_uri),
            Err(AppError::Forbidden)
        ));
        let mut prefixed_manifest_digest = room.clone();
        prefixed_manifest_digest["artifact_manifests"][0]["objects"][0]["sha256"] =
            Value::String(digest.clone());
        assert!(matches!(
            authorized_artifact_media(&prefixed_manifest_digest, &digest, &expected_uri),
            Err(AppError::NotFound)
        ));
        let mut prefixed_storage_digest = room.clone();
        prefixed_storage_digest["artifact_manifests"][0]["storage_locations"][0]["sha256"] =
            Value::String(digest.clone());
        assert!(matches!(
            authorized_artifact_media(&prefixed_storage_digest, &digest, &expected_uri),
            Err(AppError::Upstream)
        ));
        let mut noncanonical_media = room.clone();
        noncanonical_media["artifact_manifests"][0]["objects"][0]["media_type"] =
            Value::String("text/markdown".into());
        assert!(matches!(
            authorized_artifact_media(&noncanonical_media, &digest, &expected_uri),
            Err(AppError::Upstream)
        ));
        assert!(matches!(
            authorized_artifact_media(&room, &format!("sha256:{}", "cd".repeat(32)), &expected_uri),
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            authorized_artifact_media(&room, &artifact_sha256, &expected_uri),
            Err(AppError::Invalid(_))
        ));
    }

    fn resolved_review_authorization_fixture() -> (Value, Uuid, Uuid, Uuid, String, String, String)
    {
        use hepta_paper_raid_contracts::{
            frozen_review_authority_hash, frozen_review_bundle_hash, FrozenReviewAuthorityV1,
            FrozenReviewExecutionPlanV1, FrozenReviewExecutionPolicyV1, FrozenReviewObjectV1,
            FROZEN_REVIEW_AUTHORITY_V1, RESOLVED_FROZEN_REVIEW_BUNDLE_V1,
            REVIEW_OBJECT_DOWNLOAD_PATH_V1,
        };
        let paper_id = Uuid::from_u128(0x11111111_1111_4111_8111_111111111111);
        let player_id = Uuid::from_u128(0x12121212_1212_4212_8212_121212121212);
        let assignment_id = Uuid::from_u128(0x22222222_2222_4222_8222_222222222222);
        let submission_id = Uuid::from_u128(0x33333333_3333_4333_8333_333333333333);
        let object_key = "review-0004-evaluator".to_string();
        let digest = format!("sha256:{}", "a".repeat(64));
        let object = |key: &str, path: &str, role: &str, byte: char, media: &str, size| {
            FrozenReviewObjectV1 {
                object_key: key.to_string(),
                logical_path: path.to_string(),
                role: role.to_string(),
                digest: format!("sha256:{}", byte.to_string().repeat(64)),
                size_bytes: size,
                media_type: media.to_string(),
                download_path: REVIEW_OBJECT_DOWNLOAD_PATH_V1.to_string(),
            }
        };
        let authority_objects = vec![
            object(
                "review-0000-bibliography",
                "paper/references.bib",
                "bibliography",
                'd',
                "application/x-bibtex",
                111,
            ),
            object(
                "review-0001-candidate",
                "release/candidate.json",
                "candidate",
                'b',
                "application/json",
                654,
            ),
            object(
                "review-0002-claim-graph",
                "paper/claim-evidence.json",
                "claim_evidence_graph",
                'e',
                "application/json",
                222,
            ),
            object(
                "review-0003-dataset",
                "dataset/claims.json",
                "dataset",
                'c',
                "application/json",
                987,
            ),
            object(
                &object_key,
                "evaluator.py",
                "frozen_evaluator",
                'a',
                "text/x-python; charset=utf-8",
                321,
            ),
            object(
                "review-0005-paper",
                "paper/paper.md",
                "paper_source",
                'f',
                "text/markdown; charset=utf-8",
                333,
            ),
        ];
        let expires_at = "2099-01-01T00:00:00Z".to_string();
        let mut authority = FrozenReviewAuthorityV1 {
            schema: FROZEN_REVIEW_AUTHORITY_V1.to_string(),
            authority_hash: String::new(),
            assignment_id,
            paper_project_id: paper_id,
            submission_id,
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 7,
            expires_at: expires_at.clone(),
            release_candidate_hash: format!("sha256:{}", "1".repeat(64)),
            paper_bundle_hash: format!("sha256:{}", "2".repeat(64)),
            artifact_manifest_hash: format!("sha256:{}", "3".repeat(64)),
            evaluator_manifest_hash: format!("sha256:{}", "4".repeat(64)),
            dataset_manifest_hash: format!("sha256:{}", "5".repeat(64)),
            artifact_objects: authority_objects.clone(),
            execution_policy: FrozenReviewExecutionPolicyV1 {
                schema: "hepta.paper_raid.review_execution_policy.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        authority.authority_hash = frozen_review_authority_hash(&authority).unwrap();
        let mut bundle = FrozenReviewBundleV1 {
            schema: RESOLVED_FROZEN_REVIEW_BUNDLE_V1.to_string(),
            bundle_hash: String::new(),
            authority: authority.clone(),
            authority_hash: authority.authority_hash.clone(),
            assignment_id,
            paper_project_id: paper_id,
            submission_id,
            review_round: 1,
            slot: "evaluator".to_string(),
            assignment_version: 7,
            expires_at: expires_at.clone(),
            release_candidate_hash: authority.release_candidate_hash.clone(),
            paper_bundle_hash: authority.paper_bundle_hash.clone(),
            artifact_manifest_hash: authority.artifact_manifest_hash.clone(),
            evaluator_manifest_hash: authority.evaluator_manifest_hash.clone(),
            dataset_manifest_hash: authority.dataset_manifest_hash.clone(),
            objects: vec![
                FrozenReviewObjectV1 {
                    logical_path: "inputs/candidate.json".to_string(),
                    ..authority_objects[1].clone()
                },
                FrozenReviewObjectV1 {
                    logical_path: "inputs/dataset.json".to_string(),
                    ..authority_objects[3].clone()
                },
                FrozenReviewObjectV1 {
                    logical_path: "evaluator/main.py".to_string(),
                    ..authority_objects[4].clone()
                },
            ],
            execution: FrozenReviewExecutionPlanV1 {
                schema: "hepta.paper_raid.review_execution_plan.v1".to_string(),
                kind: "evaluate".to_string(),
                adapter: "python3-stdlib-v1".to_string(),
                evaluator_version: digest.clone(),
                entrypoint: "evaluator/main.py".to_string(),
                timeout_ms: 30_000,
                seed: 7,
            },
        };
        bundle.bundle_hash = frozen_review_bundle_hash(&bundle).unwrap();
        let bundle_hash = bundle.bundle_hash.clone();
        let fixture = json!({
            "paper_project_id": paper_id,
            "submission_id": submission_id,
            "status": "submission_ready",
            "release_candidate_hash": authority.release_candidate_hash,
            "paper_bundle_hash": authority.paper_bundle_hash,
            "paper_bundle": {
                "schema": "hepta.paper_raid.paper_bundle.v2",
                "release_candidate_hash": authority.release_candidate_hash,
                "paper_bundle_hash": authority.paper_bundle_hash,
                "release_candidate": {
                    "schema": "hepta.paper_raid.release_candidate.v2",
                    "paper_project_id": paper_id,
                    "artifact_manifest_hash": authority.artifact_manifest_hash,
                }
            },
            "my_assignments": [{
                "assignment_id": assignment_id,
                "paper_project_id": paper_id,
                "submission_id": submission_id,
                "player_id": player_id,
                "review_round": 1,
                "slot": "evaluator",
                "version": 7,
                "expires_at": expires_at,
                "status": "claimed"
            }],
            "resolved_frozen_review_bundle": bundle
        });
        (
            fixture,
            paper_id,
            player_id,
            assignment_id,
            bundle_hash,
            object_key,
            digest,
        )
    }

    #[test]
    fn review_artifact_authorization_is_exact_and_fail_closed() {
        let (fixture, paper_id, player_id, assignment_id, bundle_hash, object_key, digest) =
            resolved_review_authorization_fixture();
        let authorize = |value: &Value,
                         requested_bundle_hash: &str,
                         requested_key: &str,
                         requested_digest: &str| {
            authorized_review_artifact(
                value,
                ReviewArtifactAuthorization {
                    expected_paper_id: paper_id,
                    expected_player_id: player_id,
                    assignment_id,
                    bundle_hash: requested_bundle_hash,
                    object_key: requested_key,
                    digest: requested_digest,
                    audience: ReviewArtifactAudience::AgentExecutor,
                },
            )
        };
        assert_eq!(
            authorize(&fixture, &bundle_hash, &object_key, &digest)
                .expect("exact assignment-scoped object"),
            AuthorizedReviewArtifact {
                logical_path: "evaluator/main.py".to_string(),
                media_type: "text/x-python; charset=utf-8".to_string(),
                size_bytes: 321,
            }
        );
        let paper_object = &fixture["resolved_frozen_review_bundle"]["authority"]
            ["artifact_objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|object| object["role"] == "paper_source")
            .unwrap();
        let paper_key = paper_object["object_key"].as_str().unwrap();
        let paper_digest = paper_object["digest"].as_str().unwrap();
        assert_eq!(
            authorized_review_artifact(
                &fixture,
                ReviewArtifactAuthorization {
                    expected_paper_id: paper_id,
                    expected_player_id: player_id,
                    assignment_id,
                    bundle_hash: &bundle_hash,
                    object_key: paper_key,
                    digest: paper_digest,
                    audience: ReviewArtifactAudience::BrowserReviewer,
                },
            )
            .unwrap()
            .logical_path,
            "paper/paper.md"
        );
        assert!(matches!(
            authorize(&fixture, &bundle_hash, paper_key, paper_digest),
            Err(AppError::NotFound)
        ));

        let mut foreign_descriptor = fixture.clone();
        foreign_descriptor["resolved_frozen_review_bundle"]["schema"] =
            json!("hepta.paper_raid.resolved_frozen_review_bundle.v0");
        assert!(matches!(
            authorize(&foreign_descriptor, &bundle_hash, &object_key, &digest),
            Err(AppError::Upstream)
        ));

        let mut foreign_assignment = fixture.clone();
        foreign_assignment["my_assignments"][0]["paper_project_id"] = json!(Uuid::new_v4());
        assert!(matches!(
            authorize(&foreign_assignment, &bundle_hash, &object_key, &digest),
            Err(AppError::Forbidden)
        ));

        let mut foreign_manifest = fixture.clone();
        foreign_manifest["paper_bundle"]["release_candidate"]["artifact_manifest_hash"] =
            json!(format!("sha256:{}", "9".repeat(64)));
        assert!(matches!(
            authorize(&foreign_manifest, &bundle_hash, &object_key, &digest),
            Err(AppError::Forbidden)
        ));

        let mut expired_assignment = fixture.clone();
        expired_assignment["my_assignments"][0]["status"] = json!("expired");
        assert!(matches!(
            authorize(&expired_assignment, &bundle_hash, &object_key, &digest),
            Err(AppError::Forbidden)
        ));

        let mut ambiguous_assignment = fixture.clone();
        let duplicate = ambiguous_assignment["my_assignments"][0].clone();
        ambiguous_assignment["my_assignments"]
            .as_array_mut()
            .expect("assignment array")
            .push(duplicate);
        assert!(matches!(
            authorize(&ambiguous_assignment, &bundle_hash, &object_key, &digest),
            Err(AppError::Forbidden)
        ));

        let mut foreign_transport = fixture.clone();
        foreign_transport["resolved_frozen_review_bundle"]["objects"][0]["download_path"] =
            json!("https://attacker.invalid/object");
        assert!(matches!(
            authorize(&foreign_transport, &bundle_hash, &object_key, &digest),
            Err(AppError::Upstream)
        ));

        for (field, value) in [
            ("object_key", json!("../review-object-a")),
            ("logical_path", json!("../secret.py")),
            ("logical_path", json!("evaluator/secret\n.py")),
            ("role", json!("paper_source")),
            ("size_bytes", json!(0)),
            ("media_type", json!("text/html")),
        ] {
            let mut unsafe_object = fixture.clone();
            unsafe_object["resolved_frozen_review_bundle"]["objects"][0][field] = value;
            let requested_key = unsafe_object["resolved_frozen_review_bundle"]["objects"][0]
                ["object_key"]
                .as_str()
                .expect("object key");
            assert!(matches!(
                authorize(&unsafe_object, &bundle_hash, requested_key, &digest),
                Err(AppError::Upstream)
            ));
        }

        assert!(matches!(
            authorize(&fixture, &bundle_hash, "review-object-missing", &digest,),
            Err(AppError::NotFound)
        ));
        assert!(matches!(
            authorize(&fixture, "sha256:not-a-digest", &object_key, &digest,),
            Err(AppError::Invalid(_))
        ));
    }

    #[test]
    fn review_artifact_presentation_is_strict_and_responses_are_sandboxed() {
        let assignment_id = Uuid::new_v4();
        let query: ReviewArtifactQuery = serde_json::from_value(json!({
            "assignment_id": assignment_id,
            "bundle_hash": format!("sha256:{}", "d".repeat(64)),
            "object_key": "review-object-a",
            "presentation": "inline"
        }))
        .expect("strict inline presentation");
        assert_eq!(query.presentation, Some(ReviewArtifactPresentation::Inline));

        let default_query: ReviewArtifactQuery = serde_json::from_value(json!({
            "assignment_id": assignment_id,
            "bundle_hash": format!("sha256:{}", "d".repeat(64)),
            "object_key": "review-object-a"
        }))
        .expect("legacy attachment default");
        assert_eq!(
            default_query
                .presentation
                .unwrap_or(ReviewArtifactPresentation::Attachment),
            ReviewArtifactPresentation::Attachment
        );

        for hostile in [
            json!({
                "assignment_id": assignment_id,
                "bundle_hash": format!("sha256:{}", "d".repeat(64)),
                "object_key": "review-object-a",
                "presentation": "preview"
            }),
            json!({
                "assignment_id": assignment_id,
                "bundle_hash": format!("sha256:{}", "d".repeat(64)),
                "object_key": "review-object-a",
                "presentation": "inline",
                "redirect": "https://attacker.invalid"
            }),
        ] {
            assert!(serde_json::from_value::<ReviewArtifactQuery>(hostile).is_err());
        }

        let response = review_artifact_browser_response(
            b"{}".to_vec(),
            "application/json",
            "results/final\" report.json",
            ReviewArtifactPresentation::Inline,
        )
        .expect("safe inline response");
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "inline; filename=\"final__report.json\""
        );
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            "sandbox; default-src 'none'; frame-ancestors 'none'"
        );
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(response.headers()[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(
            response.headers()[HeaderName::from_static("referrer-policy")],
            "no-referrer"
        );

        let response = review_artifact_browser_response(
            b"paper".to_vec(),
            "text/markdown; charset=utf-8",
            "paper/final.md",
            ReviewArtifactPresentation::Attachment,
        )
        .expect("safe attachment response");
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"final.md\""
        );
    }

    #[tokio::test]
    async fn streaming_artifact_body_enforces_cap_and_accepts_empty() {
        assert_eq!(
            collect_limited_body(Body::from("paper"), 5)
                .await
                .expect("bounded body"),
            b"paper"
        );
        assert!(collect_limited_body(Body::from("oversized"), 4)
            .await
            .is_err());
        assert_eq!(
            collect_limited_body(Body::empty(), 4)
                .await
                .expect("zero-byte body"),
            Vec::<u8>::new()
        );
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
        let duplicate_active = json!([
            {
                "binding_id": binding_id,
                "player_id": player_id,
                "agent_id": "did:trnm:agent:alpha",
                "status": "active"
            },
            {
                "binding_id": Uuid::new_v4(),
                "player_id": player_id,
                "agent_id": "did:trnm:agent:beta",
                "status": "active"
            }
        ]);
        assert!(!has_active_agent_binding(&duplicate_active, player_id)
            .expect("multiple active bindings are ambiguous"));

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
