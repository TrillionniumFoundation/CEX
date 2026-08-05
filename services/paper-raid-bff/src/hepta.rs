use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use hepta_paper_raid_contracts::{
    canonical_json_bytes, sha256_digest, sign_consumer_user_assertion,
    ConsumerUserAssertionClaimV2, CONSUMER_USER_ASSERTION_V2,
};
use reqwest::{header, Client, Method, StatusCode};
use serde::Deserialize;
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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandName {
    CreateHumanPlayer,
    RotateHumanSigningKey,
    RevokeHumanSigningKey,
    CreateAgentBinding,
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
    QueueMatchmaking,
    SubmitAgentProposal,
    RecordHumanDecision,
    RegisterArtifact,
    SubmitReview,
    SubmitReproduction,
    SubmitAppeal,
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

#[derive(Debug, Clone)]
struct Route {
    method: Method,
    path: String,
    operation: &'static str,
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
            operation,
        };
        Ok(match self.command {
            CommandName::CreateHumanPlayer => {
                post("/v2/hepta/players".into(), "create_human_player_v2")
            }
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
            CommandName::QueueMatchmaking
            | CommandName::SubmitAgentProposal
            | CommandName::RecordHumanDecision
            | CommandName::RegisterArtifact
            | CommandName::SubmitReview
            | CommandName::SubmitReproduction
            | CommandName::SubmitAppeal => {
                return Err(AppError::Unavailable("hepta_future_paper_raid_adapter"));
            }
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
                operation: "get_joint_paper_submission_v2",
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
            canonical_path: route.path.clone(),
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
        let url = join_exact(&self.base, &route.path).map_err(AppError::Invalid)?;
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

fn business_request_hash(route: &Route, body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(route.method.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(route.path.as_bytes());
    hasher.update([0]);
    hasher.update(body);
    hasher.finalize().into()
}

fn read_json(response: UpstreamResponse) -> Result<Value, AppError> {
    match response.status {
        200..=299 => response.json(),
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
        extract::State,
        http::{HeaderMap, StatusCode as AxumStatus},
        response::{IntoResponse, Response},
        routing::post,
        Json, Router,
    };
    use hepta_paper_raid_contracts::{
        verify_consumer_user_assertion_signature, SignedConsumerUserAssertionV2,
    };
    use std::sync::{Arc, Mutex};

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
    fn future_contracts_fail_closed() {
        let command = BrowserCommand {
            command: CommandName::SubmitAgentProposal,
            resource_id: Some(Uuid::new_v4()),
            child_id: None,
            session_id: None,
            idempotency_key: Uuid::new_v4(),
            payload: Value::Null,
        };
        assert!(matches!(command.route(), Err(AppError::Unavailable(_))));
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
        let cases = [
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
        ];
        for (command, expected_path, expected_operation) in cases {
            let route = command.route().expect("exact route");
            assert_eq!(route.path, expected_path);
            assert_eq!(route.operation, expected_operation);
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

        let captured = captured.lock().expect("mock lock");
        assert_eq!(captured.calls, 1);
        assert_eq!(captured.bodies, vec![body]);
        let mut tampered = captured.assertions[0].clone();
        tampered.claim.operation = "tampered_operation".into();
        assert!(
            verify_consumer_user_assertion_signature(&tampered, &signing_key.verifying_key())
                .is_err()
        );
    }
}
