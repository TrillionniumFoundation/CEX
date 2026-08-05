use std::time::Duration;

use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

use crate::{config::AlphaIdentity, error::AppError};

const MAX_ARCHIVE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSessionAccess {
    pub logical_session_id: String,
    pub authorization_id: Uuid,
}

#[derive(Debug, Serialize)]
struct ArchiveRequest<'a> {
    schema: &'static str,
    logical_session_id: &'a str,
    after_sequence: u64,
    limit: u32,
    authorization_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RpcEnvelope {
    payload: String,
}

#[derive(Clone)]
pub struct NakamaArchiveClient {
    client: Client,
    base: Url,
    http_key: String,
}

impl NakamaArchiveClient {
    pub fn new(base: Url, http_key: String) -> Result<Self, String> {
        if base.path() != "/" && !base.path().is_empty() {
            return Err("Nakama base URL must not contain a path".to_string());
        }
        if http_key.len() < 8 || http_key.len() > 256 {
            return Err("Nakama HTTP key length is invalid".to_string());
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(8))
            .user_agent("paper-raid-bff/0.1")
            .build()
            .map_err(|error| format!("cannot build Nakama client: {error}"))?;
        Ok(Self {
            client,
            base,
            http_key,
        })
    }

    pub async fn ready(&self) -> bool {
        let mut url = self.base.clone();
        url.set_path("/healthcheck");
        url.set_query(None);
        url.set_fragment(None);
        let Ok(response) = self.client.get(url).send().await else {
            return false;
        };
        if response.status() != StatusCode::OK || !strict_json(response.headers()) {
            return false;
        }
        limited_body(response, 64 * 1024).await.is_ok()
    }

    pub async fn archive(
        &self,
        access: &MemberSessionAccess,
        after_sequence: u64,
    ) -> Result<Value, AppError> {
        validate_session_id(&access.logical_session_id)?;
        let mut url = self.base.clone();
        url.set_path("/v2/rpc/trnm_research_session_archive_v1");
        url.query_pairs_mut()
            .clear()
            .append_pair("http_key", &self.http_key);
        url.set_fragment(None);
        let runtime_payload = serde_json::to_string(&ArchiveRequest {
            schema: "trnm.nakama.research-session.get-archive.v1",
            logical_session_id: &access.logical_session_id,
            after_sequence,
            limit: 500,
            authorization_id: access.authorization_id,
        })
        .map_err(|_| AppError::Internal)?;
        let response = self
            .client
            .post(url)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .json(&runtime_payload)
            .send()
            .await
            .map_err(|_| AppError::Unavailable("nakama"))?;
        if response.status().is_redirection() || !strict_json(response.headers()) {
            return Err(AppError::Upstream);
        }
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(AppError::Forbidden);
        }
        if !response.status().is_success() {
            return Err(AppError::Unavailable("nakama"));
        }
        let bytes = limited_body(response, MAX_ARCHIVE_BYTES).await?;
        let envelope: RpcEnvelope =
            serde_json::from_slice(&bytes).map_err(|_| AppError::Upstream)?;
        if envelope.payload.len() > MAX_ARCHIVE_BYTES {
            return Err(AppError::Upstream);
        }
        serde_json::from_str(&envelope.payload).map_err(|_| AppError::Upstream)
    }
}

/// Extracts only the logged-in member's per-session authorization from a
/// Hepta-owned read model. A query parameter or deployment secret can never
/// select this value.
pub fn member_session_access(
    paper_read_model: &Value,
    identity: &AlphaIdentity,
) -> Option<MemberSessionAccess> {
    let session = paper_read_model.get("research_session")?;
    let logical_session_id = session.get("logical_session_id")?.as_str()?.to_string();
    let members = session.get("members")?.as_array()?;
    let member = members.iter().find(|member| {
        member.get("subject_id").and_then(Value::as_str) == Some(identity.subject_id.as_str())
            && member
                .get("player_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(identity.player_id)
    })?;
    let authorization_id = Uuid::parse_str(member.get("authorization_id")?.as_str()?).ok()?;
    Some(MemberSessionAccess {
        logical_session_id,
        authorization_id,
    })
}

fn validate_session_id(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 128
        || value.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
        })
    {
        return Err(AppError::Invalid("logical_session_id is invalid".into()));
    }
    Ok(())
}

fn strict_json(headers: &header::HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            matches!(
                value,
                "application/json" | "application/json; charset=utf-8"
            )
        })
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
        extract::{Query, State},
        http::HeaderMap,
        response::{IntoResponse, Redirect, Response},
        routing::post,
        Json, Router,
    };
    use std::{collections::HashMap, sync::Arc};
    use tokio::sync::Mutex;

    #[test]
    fn access_is_bound_to_current_member() {
        let identity =
            AlphaIdentity::test_identity("opaque-subject-a", Uuid::new_v4(), Uuid::new_v4());
        let other = Uuid::new_v4();
        let own_authorization = Uuid::new_v4();
        let model = serde_json::json!({
            "research_session": {
                "logical_session_id": "paper.raid:one",
                "members": [
                    {"subject_id":"other", "player_id":other, "authorization_id":Uuid::new_v4()},
                    {"subject_id":identity.subject_id, "player_id":identity.player_id, "authorization_id":own_authorization}
                ]
            }
        });
        let access = member_session_access(&model, &identity).expect("member access");
        assert_eq!(access.authorization_id, own_authorization);
    }

    #[derive(Default)]
    struct CapturedWire {
        query_key: String,
        body: String,
        content_type: String,
    }

    async fn exact_wire_handler(
        State(captured): State<Arc<Mutex<CapturedWire>>>,
        Query(query): Query<HashMap<String, String>>,
        headers: HeaderMap,
        body: String,
    ) -> Response {
        let mut captured = captured.lock().await;
        captured.query_key = query.get("http_key").cloned().unwrap_or_default();
        captured.content_type = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        captured.body = body;
        Json(serde_json::json!({
            "payload": "{\"events\":[{\"sequence\":1}]}"
        }))
        .into_response()
    }

    async fn spawn(router: Router) -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock");
        let address = listener.local_addr().expect("mock address");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("mock server");
        });
        Url::parse(&format!("http://{address}")).expect("mock URL")
    }

    #[tokio::test]
    async fn exact_rest_rpc_wire_is_double_json_and_query_encoded() {
        let captured = Arc::new(Mutex::new(CapturedWire::default()));
        let base = spawn(
            Router::new()
                .route(
                    "/v2/rpc/trnm_research_session_archive_v1",
                    post(exact_wire_handler),
                )
                .with_state(captured.clone()),
        )
        .await;
        let http_key = "alpha key+/with?symbols&";
        let client = NakamaArchiveClient::new(base, http_key.into()).expect("client");
        let authorization_id = Uuid::new_v4();
        let result = client
            .archive(
                &MemberSessionAccess {
                    logical_session_id: "paper.raid:wire".into(),
                    authorization_id,
                },
                7,
            )
            .await
            .expect("archive");
        assert_eq!(result["events"][0]["sequence"], 1);
        let captured = captured.lock().await;
        assert_eq!(captured.query_key, http_key);
        assert_eq!(captured.content_type, "application/json");
        let inner: String = serde_json::from_str(&captured.body).expect("outer JSON string");
        let payload: Value = serde_json::from_str(&inner).expect("runtime payload");
        assert_eq!(
            payload["schema"],
            "trnm.nakama.research-session.get-archive.v1"
        );
        assert_eq!(payload["authorization_id"], authorization_id.to_string());
        assert_eq!(payload["after_sequence"], 7);
        assert!(payload.get("operator_credential").is_none());
    }

    async fn redirect_handler() -> Redirect {
        Redirect::temporary("/somewhere-else")
    }

    async fn wrong_type_handler() -> Response {
        (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain")], "{}").into_response()
    }

    async fn bad_envelope_handler() -> Json<Value> {
        Json(serde_json::json!({"payload":"{}", "unexpected":true}))
    }

    async fn oversized_handler() -> Json<Value> {
        Json(serde_json::json!({"payload":"x".repeat(MAX_ARCHIVE_BYTES + 1)}))
    }

    #[tokio::test]
    async fn rejects_redirect_content_type_bad_envelope_and_body_cap() {
        let access = MemberSessionAccess {
            logical_session_id: "paper.raid:negative".into(),
            authorization_id: Uuid::new_v4(),
        };
        for handler in [
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(redirect_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(wrong_type_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(bad_envelope_handler),
            ),
            Router::new().route(
                "/v2/rpc/trnm_research_session_archive_v1",
                post(oversized_handler),
            ),
        ] {
            let base = spawn(handler).await;
            let client = NakamaArchiveClient::new(base, "alpha-http-key".into()).expect("client");
            assert!(client.archive(&access, 0).await.is_err());
        }
    }
}
