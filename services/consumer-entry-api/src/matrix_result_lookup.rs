use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use consumer_entry_api::ConsumerEntryConfig;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::Path,
    time::Duration,
};

type HmacSha256 = Hmac<Sha256>;

const LOOKUP_PATH: &str = "/v1/matrix/messages/result";
const MAX_LOOKUP_BODY_BYTES: usize = 16 * 1024;
const MAX_ASSERTION_BYTES: usize = 8 * 1024;
const MAX_REPLAY_STORE_BYTES: u64 = 32 * 1024 * 1024;
const LOOKUP_SOURCE_KIND: &str = "matrix_result_lookup";

#[derive(Clone)]
struct ResultLookupState {
    config: ConsumerEntryConfig,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MatrixResultLookupRequest {
    event_id: String,
    matrix_user_id: String,
    room_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupSessionClaims {
    version: u32,
    issuer: String,
    key_id: Option<String>,
    subject: String,
    source_kind: String,
    audience: Option<String>,
    request_fingerprint: Option<String>,
    room_id: Option<String>,
    session_id: Option<String>,
    org_id: Option<String>,
    account_id: Option<String>,
    issued_at_epoch: i64,
    expires_at_epoch: i64,
}

#[derive(Debug, Deserialize)]
struct ReplayStore {
    seen: HashMap<String, ReplayEntry>,
}

#[derive(Debug, Deserialize)]
struct ReplayEntry {
    seen_at_epoch: i64,
    response: Option<Value>,
}

pub fn router(config: ConsumerEntryConfig) -> Router {
    Router::new()
        .route(LOOKUP_PATH, post(lookup_matrix_result))
        .layer(DefaultBodyLimit::max(MAX_LOOKUP_BODY_BYTES))
        .with_state(ResultLookupState { config })
}

async fn lookup_matrix_result(
    State(state): State<ResultLookupState>,
    headers: HeaderMap,
    Json(request): Json<MatrixResultLookupRequest>,
) -> Response {
    if validate_matrix_identifier(&request.event_id, '$').is_err()
        || validate_matrix_identifier(&request.matrix_user_id, '@').is_err()
        || validate_matrix_identifier(&request.room_id, '!').is_err()
    {
        return lookup_error(StatusCode::BAD_REQUEST, "matrix_result_lookup_invalid_identity");
    }

    if authorize_ingress(&headers, &state.config).is_err() {
        return lookup_error(
            StatusCode::UNAUTHORIZED,
            "matrix_result_lookup_ingress_auth_failed",
        );
    }
    if authorize_lookup_principal(&headers, &request, &state.config).is_err() {
        return lookup_error(
            StatusCode::UNAUTHORIZED,
            "matrix_result_lookup_principal_auth_failed",
        );
    }

    let Some(path) = state.config.replay_store_path.as_deref() else {
        return lookup_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "matrix_result_lookup_store_unconfigured",
        );
    };

    let store = match read_replay_store(path) {
        Ok(store) => store,
        Err(_) => {
            tokio::time::sleep(Duration::from_millis(10)).await;
            match read_replay_store(path) {
                Ok(store) => store,
                Err(_) => {
                    return lookup_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "matrix_result_lookup_store_unavailable",
                    )
                }
            }
        }
    };

    let cache_key = format!("matrix-event:{}", request.event_id);
    let Some(entry) = store.seen.get(&cache_key) else {
        return lookup_error(StatusCode::NOT_FOUND, "matrix_result_lookup_not_found");
    };

    let now = Utc::now().timestamp();
    let max_age = i64::try_from(state.config.replay_window_secs).unwrap_or(i64::MAX);
    if entry.seen_at_epoch > now + state.config.session_auth_max_clock_skew_secs as i64
        || now.saturating_sub(entry.seen_at_epoch) > max_age
    {
        return lookup_error(StatusCode::GONE, "matrix_result_lookup_expired");
    }

    let Some(response) = entry.response.as_ref() else {
        return lookup_error(
            StatusCode::CONFLICT,
            "matrix_result_lookup_outcome_not_recorded",
        );
    };
    if validate_cached_result(response, &request).is_err() {
        return lookup_error(
            StatusCode::CONFLICT,
            "matrix_result_lookup_identity_mismatch",
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "schema": "cex.matrix.result-lookup.v1",
            "resolved": true,
            "event_id": request.event_id,
            "matrix_user_id": request.matrix_user_id,
            "room_id": request.room_id,
            "seen_at_epoch": entry.seen_at_epoch,
            "response": response,
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

fn authorize_ingress(headers: &HeaderMap, config: &ConsumerEntryConfig) -> Result<(), ()> {
    let expected = config.ingress_token.as_deref().ok_or(())?;
    let supplied = headers
        .get("x-entry-token")
        .and_then(|value| value.to_str().ok())
        .ok_or(())?;
    if constant_time_eq(expected.as_bytes(), supplied.as_bytes()) {
        Ok(())
    } else {
        Err(())
    }
}

fn authorize_lookup_principal(
    headers: &HeaderMap,
    request: &MatrixResultLookupRequest,
    config: &ConsumerEntryConfig,
) -> Result<(), ()> {
    let assertion = headers
        .get("x-cex-user-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= MAX_ASSERTION_BYTES)
        .ok_or(())?;
    let supplied_signature = headers
        .get("x-cex-user-session-signature")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or(())?;

    let assertion_bytes = URL_SAFE_NO_PAD.decode(assertion).map_err(|_| ())?;
    if assertion_bytes.len() > MAX_ASSERTION_BYTES {
        return Err(());
    }
    let claims: LookupSessionClaims =
        serde_json::from_slice(&assertion_bytes).map_err(|_| ())?;

    if claims.version != 1
        || claims.source_kind != LOOKUP_SOURCE_KIND
        || claims.subject != request.matrix_user_id
        || claims.room_id.as_deref() != Some(request.room_id.as_str())
        || claims.session_id.is_some()
        || claims.org_id.is_some()
        || claims.account_id.is_some()
        || claims.request_fingerprint.as_deref()
            != Some(lookup_request_fingerprint(request).as_str())
    {
        return Err(());
    }

    let audience = config
        .session_auth_expected_audience
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(())?;
    if claims.audience.as_deref() != Some(audience)
        || config.session_auth_allowed_issuers.is_empty()
        || !config
            .session_auth_allowed_issuers
            .iter()
            .any(|issuer| issuer == &claims.issuer)
    {
        return Err(());
    }

    let now = Utc::now().timestamp();
    let skew = i64::try_from(config.session_auth_max_clock_skew_secs).map_err(|_| ())?;
    let max_ttl = i64::try_from(config.session_auth_max_ttl_secs).map_err(|_| ())?;
    if claims.issued_at_epoch > now.saturating_add(skew)
        || claims.expires_at_epoch <= claims.issued_at_epoch
        || claims.expires_at_epoch < now.saturating_sub(skew)
        || claims
            .expires_at_epoch
            .saturating_sub(claims.issued_at_epoch)
            > max_ttl
    {
        return Err(());
    }

    let secret = resolve_lookup_secret(config, &claims).ok_or(())?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| ())?;
    mac.update(assertion.as_bytes());
    let expected_signature = mac.finalize().into_bytes();
    let supplied_signature = URL_SAFE_NO_PAD
        .decode(supplied_signature)
        .map_err(|_| ())?;
    if supplied_signature.len() != expected_signature.len()
        || !constant_time_eq(&supplied_signature, expected_signature.as_slice())
    {
        return Err(());
    }
    Ok(())
}

fn resolve_lookup_secret(
    config: &ConsumerEntryConfig,
    claims: &LookupSessionClaims,
) -> Option<String> {
    if let Some(key_id) = claims.key_id.as_deref() {
        if let Some(secret) = config
            .session_auth_issuer_keys
            .get(&claims.issuer)
            .and_then(|keys| keys.get(key_id))
            .filter(|secret| !secret.is_empty())
        {
            return Some(secret.clone());
        }
        if let Some(entry) = config.session_auth_issuer_registry.get(&claims.issuer) {
            let value = serde_json::to_value(entry).ok()?;
            if let Some(secret) = value
                .get("keys")
                .and_then(Value::as_object)
                .and_then(|keys| keys.get(key_id))
                .and_then(Value::as_str)
                .filter(|secret| !secret.is_empty())
            {
                return Some(secret.to_string());
            }
        }
    }

    config
        .session_auth_issuer_secrets
        .get(&claims.issuer)
        .filter(|secret| !secret.is_empty())
        .cloned()
        .or_else(|| {
            config
                .session_auth_secret
                .as_ref()
                .filter(|secret| !secret.is_empty())
                .cloned()
        })
}

fn read_replay_store(path: &str) -> Result<ReplayStore, ()> {
    let path = Path::new(path);
    let link_metadata = std::fs::symlink_metadata(path).map_err(|_| ())?;
    if link_metadata.file_type().is_symlink()
        || !link_metadata.file_type().is_file()
        || link_metadata.len() > MAX_REPLAY_STORE_BYTES
    {
        return Err(());
    }

    let mut file = File::open(path).map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > MAX_REPLAY_STORE_BYTES {
        return Err(());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_REPLAY_STORE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_REPLAY_STORE_BYTES {
        return Err(());
    }
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn validate_cached_result(
    response: &Value,
    request: &MatrixResultLookupRequest,
) -> Result<(), ()> {
    let source = response.get("source").and_then(Value::as_object).ok_or(())?;
    if source.get("kind").and_then(Value::as_str) != Some("matrix_message")
        || source.get("matrix_user_id").and_then(Value::as_str)
            != Some(request.matrix_user_id.as_str())
        || source.get("room_id").and_then(Value::as_str) != Some(request.room_id.as_str())
        || source.get("event_id").and_then(Value::as_str) != Some(request.event_id.as_str())
    {
        return Err(());
    }

    let identity_scope = source
        .get("identity_scope")
        .and_then(Value::as_object)
        .ok_or(())?;
    if identity_scope.get("user_id").and_then(Value::as_str)
        != Some(request.matrix_user_id.as_str())
        || identity_scope.get("room_id").and_then(Value::as_str)
            != Some(request.room_id.as_str())
    {
        return Err(());
    }

    let task_id = response
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or(())?;
    if let Some(invocation_id) = response
        .get("raw")
        .and_then(|raw| raw.get("invocation_id"))
        .and_then(Value::as_str)
    {
        if invocation_id != task_id {
            return Err(());
        }
    }
    Ok(())
}

fn lookup_request_fingerprint(request: &MatrixResultLookupRequest) -> String {
    let mut hasher = Sha256::new();
    for value in [
        "cex.matrix.result-lookup.v1",
        request.event_id.as_str(),
        request.matrix_user_id.as_str(),
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

fn validate_matrix_identifier(value: &str, prefix: char) -> Result<(), ()> {
    if value.len() < 2
        || value.len() > 512
        || !value.starts_with(prefix)
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        Err(())
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

fn lookup_error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(json!({
            "schema": "cex.matrix.result-lookup.error.v1",
            "resolved": false,
            "error": code,
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MatrixResultLookupRequest {
        MatrixResultLookupRequest {
            event_id: "$event".to_string(),
            matrix_user_id: "@alice:example".to_string(),
            room_id: "!room:example".to_string(),
        }
    }

    #[test]
    fn fingerprint_is_bound_to_every_identity_component() {
        let base = request();
        let expected = lookup_request_fingerprint(&base);
        for candidate in [
            MatrixResultLookupRequest {
                event_id: "$other".to_string(),
                ..request()
            },
            MatrixResultLookupRequest {
                matrix_user_id: "@other:example".to_string(),
                ..request()
            },
            MatrixResultLookupRequest {
                room_id: "!other:example".to_string(),
                ..request()
            },
        ] {
            assert_ne!(lookup_request_fingerprint(&candidate), expected);
        }
    }

    #[test]
    fn cached_result_requires_exact_source_and_task_identity() {
        let request = request();
        let valid = json!({
            "task_id": "task-1",
            "source": {
                "kind": "matrix_message",
                "matrix_user_id": request.matrix_user_id,
                "room_id": request.room_id,
                "event_id": request.event_id,
                "identity_scope": {
                    "user_id": "@alice:example",
                    "room_id": "!room:example"
                }
            },
            "raw": {"invocation_id": "task-1"}
        });
        assert_eq!(validate_cached_result(&valid, &request), Ok(()));

        for pointer in ["matrix_user_id", "room_id", "event_id"] {
            let mut invalid = valid.clone();
            invalid["source"][pointer] = json!("mismatch");
            assert_eq!(validate_cached_result(&invalid, &request), Err(()));
        }
        let mut invalid = valid;
        invalid["raw"]["invocation_id"] = json!("task-2");
        assert_eq!(validate_cached_result(&invalid, &request), Err(()));
    }
}
