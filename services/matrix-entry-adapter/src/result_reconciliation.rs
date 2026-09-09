use super::implementation::MatrixAdapterConfig;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{redirect::Policy, Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

type HmacSha256 = Hmac<Sha256>;
type ValidationResult<T> = Result<T, ValidationError>;

const RECONCILIATION_PATH: &str = "/v1/matrix/results/lookup";
const CONSUMER_LOOKUP_PATH: &str = "/v1/matrix/messages/result";
const LOOKUP_SOURCE_KIND: &str = "matrix_result_lookup";
const DELIVERY_FINGERPRINT_DOMAIN: &str = "cex.matrix.adapter-result-delivery.v1";
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
struct ReconciliationState {
    http: Client,
    adapter_ingress_token: Option<String>,
    consumer_entry_base_url: String,
    consumer_entry_api_key: Option<String>,
    consumer_entry_ingress_token: Option<String>,
    signing_secret: Option<String>,
    signing_key_id: Option<String>,
    issuer: String,
    audience: String,
    ttl_secs: u64,
}

#[derive(Clone, Copy, Debug)]
struct ValidationError;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MatrixResultReconciliationRequest {
    delivery_id: String,
    event_id: String,
    room_id: String,
    sender: String,
    payload_sha256: String,
    request_fingerprint: String,
}

#[derive(Debug, Serialize)]
struct LookupSessionClaims<'a> {
    version: u32,
    issuer: &'a str,
    key_id: Option<&'a str>,
    subject: &'a str,
    source_kind: &'static str,
    audience: Option<&'a str>,
    request_fingerprint: Option<String>,
    room_id: Option<&'a str>,
    session_id: Option<&'a str>,
    org_id: Option<&'a str>,
    account_id: Option<&'a str>,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

pub fn router(config: &MatrixAdapterConfig) -> Router {
    let http = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .expect("matrix result reconciliation HTTP client must build");
    let state = ReconciliationState {
        http,
        adapter_ingress_token: config.ingress_token.clone(),
        consumer_entry_base_url: config.consumer_entry_base_url.clone(),
        consumer_entry_api_key: config.consumer_entry_api_key.clone(),
        consumer_entry_ingress_token: config.consumer_entry_ingress_token.clone(),
        signing_secret: config.consumer_entry_session_auth_secret.clone(),
        signing_key_id: config.consumer_entry_session_auth_key_id.clone(),
        issuer: config.consumer_entry_session_auth_issuer.clone(),
        audience: config.consumer_entry_session_auth_audience.clone(),
        ttl_secs: config.consumer_entry_session_auth_ttl_secs,
    };

    Router::new()
        .route(RECONCILIATION_PATH, post(reconcile_matrix_result))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state)
}

async fn reconcile_matrix_result(
    State(state): State<ReconciliationState>,
    headers: HeaderMap,
    Json(request): Json<MatrixResultReconciliationRequest>,
) -> Response {
    let expected_fingerprint = lookup_request_fingerprint(&request);
    if validate_delivery_id(&request.delivery_id).is_err()
        || validate_matrix_identifier(&request.event_id, '$').is_err()
        || validate_matrix_identifier(&request.room_id, '!').is_err()
        || validate_matrix_identifier(&request.sender, '@').is_err()
        || validate_sha256(&request.payload_sha256).is_err()
        || validate_sha256(&request.request_fingerprint).is_err()
        || request.request_fingerprint != expected_fingerprint
    {
        return reconciliation_error(
            StatusCode::BAD_REQUEST,
            "matrix_result_reconciliation_invalid_delivery_binding",
            Some(&request),
        );
    }

    let Some(expected_token) = state.adapter_ingress_token.as_deref() else {
        return reconciliation_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "matrix_result_reconciliation_ingress_unconfigured",
            Some(&request),
        );
    };
    let supplied_token = headers
        .get("x-entry-token")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !constant_time_eq(expected_token.as_bytes(), supplied_token.as_bytes()) {
        return reconciliation_error(
            StatusCode::UNAUTHORIZED,
            "matrix_result_reconciliation_auth_failed",
            Some(&request),
        );
    }

    let Some(consumer_ingress_token) = state.consumer_entry_ingress_token.as_deref() else {
        return reconciliation_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "matrix_result_reconciliation_consumer_ingress_unconfigured",
            Some(&request),
        );
    };
    let Some(signing_secret) = state.signing_secret.as_deref() else {
        return reconciliation_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "matrix_result_reconciliation_signing_unconfigured",
            Some(&request),
        );
    };

    let lookup_body = json!({
        "delivery_id": &request.delivery_id,
        "event_id": &request.event_id,
        "matrix_user_id": &request.sender,
        "room_id": &request.room_id,
        "payload_sha256": &request.payload_sha256,
        "request_fingerprint": &request.request_fingerprint,
    });
    let (assertion, signature) = match sign_lookup_assertion(&state, &request, signing_secret) {
        Ok(value) => value,
        Err(_) => {
            return reconciliation_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "matrix_result_reconciliation_signing_failed",
                Some(&request),
            )
        }
    };
    let url = match consumer_lookup_url(&state.consumer_entry_base_url) {
        Ok(url) => url,
        Err(_) => {
            return reconciliation_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "matrix_result_reconciliation_consumer_url_invalid",
                Some(&request),
            )
        }
    };

    let mut outbound = state
        .http
        .post(url)
        .header("x-entry-token", consumer_ingress_token)
        .header("x-cex-user-session", assertion)
        .header("x-cex-user-session-signature", signature)
        .json(&lookup_body);
    if let Some(api_key) = state.consumer_entry_api_key.as_deref() {
        outbound = outbound.header("x-api-key", api_key);
    }

    let response = match outbound.send().await {
        Ok(response) => response,
        Err(_) => {
            return reconciliation_error(
                StatusCode::BAD_GATEWAY,
                "matrix_result_reconciliation_consumer_unavailable",
                Some(&request),
            )
        }
    };
    let status = response.status();
    let body = match read_bounded_body(response).await {
        Ok(body) => body,
        Err(_) => {
            return reconciliation_error(
                StatusCode::BAD_GATEWAY,
                "matrix_result_reconciliation_consumer_response_unverified",
                Some(&request),
            )
        }
    };
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            return reconciliation_error(
                StatusCode::BAD_GATEWAY,
                "matrix_result_reconciliation_consumer_response_invalid",
                Some(&request),
            )
        }
    };

    if !status.is_success() {
        let mapped = match status {
            reqwest::StatusCode::NOT_FOUND => StatusCode::NOT_FOUND,
            reqwest::StatusCode::GONE => StatusCode::GONE,
            reqwest::StatusCode::CONFLICT => StatusCode::CONFLICT,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
                StatusCode::BAD_GATEWAY
            }
            _ => StatusCode::BAD_GATEWAY,
        };
        return reconciliation_error(
            mapped,
            "matrix_result_reconciliation_unresolved",
            Some(&request),
        );
    }

    let forwarded = match validate_lookup_response(&value, &request) {
        Ok(response) => response,
        Err(_) => {
            return reconciliation_error(
                StatusCode::CONFLICT,
                "matrix_result_reconciliation_identity_mismatch",
                Some(&request),
            )
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "accepted": true,
            "action": "task_result_reconciled",
            "delivery_id": request.delivery_id,
            "event_id": request.event_id,
            "room_id": request.room_id,
            "sender": request.sender,
            "payload_sha256": request.payload_sha256,
            "request_fingerprint": request.request_fingerprint,
            "forwarded": forwarded,
            "projected_reply": null,
            "reconciliation": {
                "schema": "cex.matrix.adapter-result-reconciliation.v2",
                "source": "consumer_entry_durable_replay",
                "read_only": true,
                "causal_binding": "delivery_payload_fingerprint"
            },
            "generated_at": Utc::now().to_rfc3339(),
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

fn sign_lookup_assertion(
    state: &ReconciliationState,
    request: &MatrixResultReconciliationRequest,
    secret: &str,
) -> ValidationResult<(String, String)> {
    if state.issuer.is_empty() || state.audience.is_empty() || state.ttl_secs == 0 {
        return Err(ValidationError);
    }
    let fingerprint = lookup_request_fingerprint(request);
    if request.request_fingerprint != fingerprint {
        return Err(ValidationError);
    }
    let issued_at_epoch = Utc::now().timestamp();
    let ttl = i64::try_from(state.ttl_secs).map_err(|_| ValidationError)?;
    let expires_at_epoch = issued_at_epoch.checked_add(ttl).ok_or(ValidationError)?;
    let claims = LookupSessionClaims {
        version: 1,
        issuer: &state.issuer,
        key_id: state.signing_key_id.as_deref(),
        subject: &request.sender,
        source_kind: LOOKUP_SOURCE_KIND,
        audience: Some(&state.audience),
        request_fingerprint: Some(fingerprint),
        room_id: Some(&request.room_id),
        session_id: None,
        org_id: None,
        account_id: None,
        issued_at_epoch,
        expires_at_epoch,
    };
    let assertion =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(|_| ValidationError)?);
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ValidationError)?;
    mac.update(assertion.as_bytes());
    let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok((assertion, signature))
}

fn lookup_request_fingerprint(request: &MatrixResultReconciliationRequest) -> String {
    let mut hasher = Sha256::new();
    for value in [
        DELIVERY_FINGERPRINT_DOMAIN,
        request.delivery_id.as_str(),
        request.payload_sha256.as_str(),
        request.event_id.as_str(),
        request.sender.as_str(),
        request.room_id.as_str(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!(
        "sha256:{}",
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn consumer_lookup_url(base: &str) -> ValidationResult<Url> {
    let mut url = Url::parse(base).map_err(|_| ValidationError)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(ValidationError);
    }
    let path = format!(
        "{}{}",
        url.path().trim_end_matches('/'),
        CONSUMER_LOOKUP_PATH
    );
    url.set_path(&path);
    Ok(url)
}

async fn read_bounded_body(mut response: reqwest::Response) -> ValidationResult<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(ValidationError);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ValidationError)? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(ValidationError);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn validate_lookup_response(
    value: &Value,
    request: &MatrixResultReconciliationRequest,
) -> ValidationResult<Value> {
    if value.get("schema").and_then(Value::as_str) != Some("cex.matrix.result-lookup.v1")
        || value.get("resolved").and_then(Value::as_bool) != Some(true)
        || value.get("delivery_id").and_then(Value::as_str) != Some(request.delivery_id.as_str())
        || value.get("event_id").and_then(Value::as_str) != Some(request.event_id.as_str())
        || value.get("matrix_user_id").and_then(Value::as_str) != Some(request.sender.as_str())
        || value.get("room_id").and_then(Value::as_str) != Some(request.room_id.as_str())
        || value.get("payload_sha256").and_then(Value::as_str)
            != Some(request.payload_sha256.as_str())
        || value.get("request_fingerprint").and_then(Value::as_str)
            != Some(request.request_fingerprint.as_str())
        || request.request_fingerprint != lookup_request_fingerprint(request)
    {
        return Err(ValidationError);
    }
    let response = value.get("response").cloned().ok_or(ValidationError)?;
    let source = response
        .get("source")
        .and_then(Value::as_object)
        .ok_or(ValidationError)?;
    if source.get("kind").and_then(Value::as_str) != Some("matrix_message")
        || source.get("event_id").and_then(Value::as_str) != Some(request.event_id.as_str())
        || source.get("matrix_user_id").and_then(Value::as_str) != Some(request.sender.as_str())
        || source.get("room_id").and_then(Value::as_str) != Some(request.room_id.as_str())
    {
        return Err(ValidationError);
    }
    let identity_scope = source
        .get("identity_scope")
        .and_then(Value::as_object)
        .ok_or(ValidationError)?;
    if identity_scope.get("user_id").and_then(Value::as_str)
        != Some(request.sender.as_str())
        || identity_scope.get("room_id").and_then(Value::as_str)
            != Some(request.room_id.as_str())
    {
        return Err(ValidationError);
    }
    response
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or(ValidationError)?;
    Ok(response)
}

fn validate_delivery_id(value: &str) -> ValidationResult<()> {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return Err(ValidationError);
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return Err(ValidationError);
            }
        } else if !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte) {
            return Err(ValidationError);
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> ValidationResult<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ValidationError);
    };
    if hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(ValidationError)
    }
}

fn validate_matrix_identifier(value: &str, prefix: char) -> ValidationResult<()> {
    if value.len() < 2
        || value.len() > 512
        || !value.starts_with(prefix)
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        Err(ValidationError)
    } else {
        Ok(())
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right.iter()) {
        difference |= left ^ right;
    }
    difference == 0
}

fn reconciliation_error(
    status: StatusCode,
    code: &'static str,
    request: Option<&MatrixResultReconciliationRequest>,
) -> Response {
    let identity = request.map(|request| {
        json!({
            "delivery_id": &request.delivery_id,
            "event_id": &request.event_id,
            "room_id": &request.room_id,
            "sender": &request.sender,
            "payload_sha256": &request.payload_sha256,
            "request_fingerprint": &request.request_fingerprint,
        })
    });
    (
        status,
        Json(json!({
            "accepted": false,
            "action": "task_result_reconciliation_held",
            "identity": identity,
            "error": code,
            "projected_reply": null,
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MatrixResultReconciliationRequest {
        let mut request = MatrixResultReconciliationRequest {
            delivery_id: "61000000-0000-4000-8000-000000000001".to_string(),
            event_id: "$event".to_string(),
            room_id: "!room:example".to_string(),
            sender: "@alice:example".to_string(),
            payload_sha256: format!("sha256:{}", "2".repeat(64)),
            request_fingerprint: String::new(),
        };
        request.request_fingerprint = lookup_request_fingerprint(&request);
        request
    }

    #[test]
    fn lookup_url_preserves_a_reviewed_reverse_proxy_prefix() {
        let url = consumer_lookup_url("https://entry.example/prefix").unwrap();
        assert_eq!(
            url.as_str(),
            "https://entry.example/prefix/v1/matrix/messages/result"
        );
        for invalid in [
            "ftp://entry.example/prefix",
            "https://user@entry.example/prefix",
            "https://entry.example/prefix?mode=unsafe",
        ] {
            assert!(consumer_lookup_url(invalid).is_err());
        }
    }

    #[test]
    fn delivery_fingerprint_changes_with_every_authority_component() {
        let expected = lookup_request_fingerprint(&request());

        let mut changed = request();
        changed.delivery_id = "61000000-0000-4000-8000-000000000002".to_string();
        assert_ne!(lookup_request_fingerprint(&changed), expected);

        let mut changed = request();
        changed.payload_sha256 = format!("sha256:{}", "3".repeat(64));
        assert_ne!(lookup_request_fingerprint(&changed), expected);

        let mut changed = request();
        changed.event_id = "$other".to_string();
        assert_ne!(lookup_request_fingerprint(&changed), expected);

        let mut changed = request();
        changed.sender = "@other:example".to_string();
        assert_ne!(lookup_request_fingerprint(&changed), expected);

        let mut changed = request();
        changed.room_id = "!other:example".to_string();
        assert_ne!(lookup_request_fingerprint(&changed), expected);
    }

    #[test]
    fn lookup_response_is_bound_to_the_exact_delivery_payload() {
        let request = request();
        let value = json!({
            "schema": "cex.matrix.result-lookup.v1",
            "resolved": true,
            "delivery_id": request.delivery_id.clone(),
            "event_id": request.event_id.clone(),
            "matrix_user_id": request.sender.clone(),
            "room_id": request.room_id.clone(),
            "payload_sha256": request.payload_sha256.clone(),
            "request_fingerprint": request.request_fingerprint.clone(),
            "response": {
                "task_id": "task-1",
                "source": {
                    "kind": "matrix_message",
                    "event_id": request.event_id.clone(),
                    "matrix_user_id": request.sender.clone(),
                    "room_id": request.room_id.clone(),
                    "identity_scope": {
                        "user_id": request.sender.clone(),
                        "room_id": request.room_id.clone()
                    }
                }
            }
        });
        assert!(validate_lookup_response(&value, &request).is_ok());
        for field in [
            "delivery_id",
            "event_id",
            "matrix_user_id",
            "room_id",
            "payload_sha256",
            "request_fingerprint",
        ] {
            let mut invalid = value.clone();
            invalid[field] = json!("mismatch");
            assert!(validate_lookup_response(&invalid, &request).is_err());
        }
    }
}
