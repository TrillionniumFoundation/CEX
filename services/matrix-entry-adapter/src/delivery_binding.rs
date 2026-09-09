use super::implementation::{MatrixAdapterConfig, RuntimeProfile};
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header::CONTENT_LENGTH, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const MATRIX_EVENT_PATH: &str = "/v1/matrix/events";
const MAX_EVENT_BODY_BYTES: usize = 1024 * 1024;
const DELIVERY_BINDING_FIELD: &str = "cex_delivery_binding";
const DELIVERY_BINDING_SCHEMA: &str = "cex.matrix.delivery-binding.v1";
const DELIVERY_BINDING_SOURCE: &str = "matrix-bot-relay-headers-v1";
const DELIVERY_FINGERPRINT_DOMAIN: &str = "cex.matrix.adapter-result-delivery.v1";

#[derive(Clone, Copy, Debug)]
pub(super) struct DeliveryBindingPolicy {
    require_headers: bool,
}

impl DeliveryBindingPolicy {
    pub(super) fn from_config(config: &MatrixAdapterConfig) -> Self {
        Self {
            require_headers: matches!(
                config.runtime_profile,
                RuntimeProfile::Beta | RuntimeProfile::Production
            ),
        }
    }
}

pub(super) async fn enforce_delivery_binding(
    State(policy): State<DeliveryBindingPolicy>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() != Method::POST || request.uri().path() != MATRIX_EVENT_PATH {
        return next.run(request).await;
    }

    let delivery_id = match unique_header_text(&request, "x-cex-delivery-id") {
        Ok(value) => value,
        Err(code) => return binding_error(StatusCode::BAD_REQUEST, code),
    };
    let payload_sha256 = match unique_header_text(&request, "x-cex-payload-sha256") {
        Ok(value) => value,
        Err(code) => return binding_error(StatusCode::BAD_REQUEST, code),
    };
    match (delivery_id.as_deref(), payload_sha256.as_deref()) {
        (None, None) if !policy.require_headers => return next.run(request).await,
        (Some(_), Some(_)) => {}
        _ => {
            return binding_error(
                StatusCode::BAD_REQUEST,
                "matrix_delivery_binding_headers_incomplete",
            )
        }
    }

    let (Some(delivery_id), Some(payload_sha256)) = (delivery_id, payload_sha256) else {
        return binding_error(
            StatusCode::BAD_REQUEST,
            "matrix_delivery_binding_headers_incomplete",
        );
    };
    if !valid_delivery_id(&delivery_id) || !valid_sha256(&payload_sha256) {
        return binding_error(
            StatusCode::BAD_REQUEST,
            "matrix_delivery_binding_headers_invalid",
        );
    }

    for header_name in ["x-idempotency-key", "idempotency-key"] {
        match unique_header_text(&request, header_name) {
            Ok(Some(value)) if value == delivery_id => {}
            Ok(_) => {
                return binding_error(
                    StatusCode::CONFLICT,
                    "matrix_delivery_idempotency_binding_mismatch",
                )
            }
            Err(code) => return binding_error(StatusCode::BAD_REQUEST, code),
        }
    }

    let (mut parts, body) = request.into_parts();
    let bytes = match to_bytes(body, MAX_EVENT_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return binding_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "matrix_delivery_binding_body_unavailable",
            )
        }
    };
    let bound = match bind_event_body(&bytes, &delivery_id, &payload_sha256) {
        Ok(bound) => bound,
        Err(code) => return binding_error(StatusCode::CONFLICT, code),
    };
    parts.headers.remove(CONTENT_LENGTH);
    next.run(Request::from_parts(parts, Body::from(bound))).await
}

fn unique_header_text(
    request: &Request,
    name: &'static str,
) -> Result<Option<String>, &'static str> {
    let mut values = request.headers().get_all(name).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err("matrix_delivery_binding_header_duplicated");
    }
    let value = first
        .to_str()
        .map_err(|_| "matrix_delivery_binding_header_invalid")?;
    if value.is_empty() || value != value.trim() {
        return Err("matrix_delivery_binding_header_invalid");
    }
    Ok(Some(value.to_string()))
}

fn bind_event_body(
    raw: &[u8],
    delivery_id: &str,
    payload_sha256: &str,
) -> Result<Vec<u8>, &'static str> {
    let mut event: Value = serde_json::from_slice(raw)
        .map_err(|_| "matrix_delivery_binding_invalid_json")?;
    let event_object = event
        .as_object_mut()
        .ok_or("matrix_delivery_binding_event_not_object")?;

    let canonical = serde_json::to_vec(event_object)
        .map_err(|_| "matrix_delivery_binding_canonicalization_failed")?;
    if sha256_prefixed(&canonical) != payload_sha256 {
        return Err("matrix_delivery_payload_hash_mismatch");
    }

    let event_id = required_identifier(event_object, "event_id", '$')?;
    let room_id = required_identifier(event_object, "room_id", '!')?;
    let matrix_user_id = required_identifier(event_object, "sender", '@')?;
    let request_fingerprint = delivery_request_fingerprint(
        delivery_id,
        payload_sha256,
        &event_id,
        &matrix_user_id,
        &room_id,
    );

    let metadata = event_object
        .entry("metadata".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if metadata.is_null() {
        *metadata = Value::Object(Map::new());
    }
    let metadata = metadata
        .as_object_mut()
        .ok_or("matrix_delivery_binding_metadata_not_object")?;
    if metadata.contains_key(DELIVERY_BINDING_FIELD) {
        return Err("matrix_delivery_binding_reserved_field_present");
    }
    metadata.insert(
        DELIVERY_BINDING_FIELD.to_string(),
        json!({
            "schema": DELIVERY_BINDING_SCHEMA,
            "source": DELIVERY_BINDING_SOURCE,
            "delivery_id": delivery_id,
            "payload_sha256": payload_sha256,
            "event_id": event_id,
            "room_id": room_id,
            "matrix_user_id": matrix_user_id,
            "request_fingerprint": request_fingerprint,
        }),
    );

    let bound = serde_json::to_vec(&event)
        .map_err(|_| "matrix_delivery_binding_serialization_failed")?;
    if bound.len() > MAX_EVENT_BODY_BYTES {
        return Err("matrix_delivery_binding_body_too_large");
    }
    Ok(bound)
}

fn required_identifier(
    event: &Map<String, Value>,
    field: &str,
    prefix: char,
) -> Result<String, &'static str> {
    let value = event
        .get(field)
        .and_then(Value::as_str)
        .ok_or("matrix_delivery_binding_identity_missing")?;
    if value.len() < 2
        || value.len() > 512
        || !value.starts_with(prefix)
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err("matrix_delivery_binding_identity_invalid");
    }
    Ok(value.to_string())
}

fn delivery_request_fingerprint(
    delivery_id: &str,
    payload_sha256: &str,
    event_id: &str,
    matrix_user_id: &str,
    room_id: &str,
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        DELIVERY_FINGERPRINT_DOMAIN,
        delivery_id,
        payload_sha256,
        event_id,
        matrix_user_id,
        room_id,
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

fn sha256_prefixed(bytes: &[u8]) -> String {
    format!(
        "sha256:{}",
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn valid_delivery_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().copied().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}

fn valid_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn binding_error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(json!({
            "accepted": false,
            "action": "matrix_delivery_binding_rejected",
            "error": code,
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};
    use tower::ServiceExt;

    const DELIVERY_ID: &str = "65000000-0000-4000-8000-000000000001";

    fn event() -> Value {
        json!({
            "event_id": "$delivery-bound-event",
            "event_type": "m.room.message",
            "room_id": "!delivery-room:example",
            "sender": "@delivery-user:example",
            "text": "/task preserve delivery identity",
            "content": {"msgtype": "m.text", "body": "/task preserve delivery identity"},
            "timestamp_ms": 1_789_000_000_000_i64,
            "metadata": {"upstream": "matrix-bot-relay"}
        })
    }

    fn event_request(value: &Value, include_binding_headers: bool) -> Request {
        let raw = serde_json::to_vec(value).unwrap();
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(MATRIX_EVENT_PATH)
            .header("content-type", "application/json");
        if include_binding_headers {
            builder = builder
                .header("x-cex-delivery-id", DELIVERY_ID)
                .header("x-cex-payload-sha256", sha256_prefixed(&raw))
                .header("x-idempotency-key", DELIVERY_ID)
                .header("idempotency-key", DELIVERY_ID);
        }
        builder.body(Body::from(raw)).unwrap()
    }

    fn test_router(policy: DeliveryBindingPolicy) -> Router {
        Router::new()
            .route(
                MATRIX_EVENT_PATH,
                post(|Json(value): Json<Value>| async move { Json(value) }),
            )
            .layer(axum::middleware::from_fn_with_state(
                policy,
                enforce_delivery_binding,
            ))
    }

    #[test]
    fn trusted_headers_become_durable_reserved_metadata() {
        let value = event();
        let raw = serde_json::to_vec(&value).unwrap();
        let payload_sha256 = sha256_prefixed(&raw);
        let bound = bind_event_body(&raw, DELIVERY_ID, &payload_sha256).unwrap();
        let bound: Value = serde_json::from_slice(&bound).unwrap();
        let binding = &bound["metadata"][DELIVERY_BINDING_FIELD];
        assert_eq!(binding["schema"], DELIVERY_BINDING_SCHEMA);
        assert_eq!(binding["source"], DELIVERY_BINDING_SOURCE);
        assert_eq!(binding["delivery_id"], DELIVERY_ID);
        assert_eq!(binding["payload_sha256"], payload_sha256);
        assert_eq!(binding["event_id"], "$delivery-bound-event");
        assert_eq!(binding["room_id"], "!delivery-room:example");
        assert_eq!(binding["matrix_user_id"], "@delivery-user:example");
        assert_eq!(
            binding["request_fingerprint"],
            delivery_request_fingerprint(
                DELIVERY_ID,
                binding["payload_sha256"].as_str().unwrap(),
                "$delivery-bound-event",
                "@delivery-user:example",
                "!delivery-room:example",
            )
        );
    }

    #[test]
    fn changed_payload_and_reserved_binding_fail_closed() {
        let value = event();
        let raw = serde_json::to_vec(&value).unwrap();
        let payload_sha256 = sha256_prefixed(&raw);

        let mut changed = value.clone();
        changed["text"] = json!("changed after durable admission");
        let changed = serde_json::to_vec(&changed).unwrap();
        assert_eq!(
            bind_event_body(&changed, DELIVERY_ID, &payload_sha256),
            Err("matrix_delivery_payload_hash_mismatch")
        );

        let mut reserved = value;
        reserved["metadata"][DELIVERY_BINDING_FIELD] = json!({"forged": true});
        let reserved = serde_json::to_vec(&reserved).unwrap();
        let reserved_hash = sha256_prefixed(&reserved);
        assert_eq!(
            bind_event_body(&reserved, DELIVERY_ID, &reserved_hash),
            Err("matrix_delivery_binding_reserved_field_present")
        );
    }

    #[tokio::test]
    async fn production_rejects_missing_headers_and_accepts_complete_binding() {
        let production = DeliveryBindingPolicy {
            require_headers: true,
        };
        let missing = test_router(production)
            .oneshot(event_request(&event(), false))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::BAD_REQUEST);

        let accepted = test_router(production)
            .oneshot(event_request(&event(), true))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::OK);
        let body = to_bytes(accepted.into_body(), MAX_EVENT_BODY_BYTES)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value["metadata"][DELIVERY_BINDING_FIELD]["delivery_id"],
            DELIVERY_ID
        );
    }

    #[tokio::test]
    async fn local_unbound_request_passes_but_partial_binding_is_rejected() {
        let local = DeliveryBindingPolicy {
            require_headers: false,
        };
        let unbound = test_router(local)
            .oneshot(event_request(&event(), false))
            .await
            .unwrap();
        assert_eq!(unbound.status(), StatusCode::OK);

        let raw = serde_json::to_vec(&event()).unwrap();
        let partial = Request::builder()
            .method(Method::POST)
            .uri(MATRIX_EVENT_PATH)
            .header("content-type", "application/json")
            .header("x-cex-delivery-id", DELIVERY_ID)
            .body(Body::from(raw))
            .unwrap();
        let rejected = test_router(local).oneshot(partial).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn duplicate_or_mismatched_idempotency_headers_fail_closed() {
        let local = DeliveryBindingPolicy {
            require_headers: false,
        };
        let raw = serde_json::to_vec(&event()).unwrap();
        let duplicated = Request::builder()
            .method(Method::POST)
            .uri(MATRIX_EVENT_PATH)
            .header("content-type", "application/json")
            .header("x-cex-delivery-id", DELIVERY_ID)
            .header(
                "x-cex-delivery-id",
                "65000000-0000-4000-8000-000000000002",
            )
            .header("x-cex-payload-sha256", sha256_prefixed(&raw))
            .header("x-idempotency-key", DELIVERY_ID)
            .header("idempotency-key", DELIVERY_ID)
            .body(Body::from(raw.clone()))
            .unwrap();
        let rejected = test_router(local).oneshot(duplicated).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);

        let mismatched = Request::builder()
            .method(Method::POST)
            .uri(MATRIX_EVENT_PATH)
            .header("content-type", "application/json")
            .header("x-cex-delivery-id", DELIVERY_ID)
            .header("x-cex-payload-sha256", sha256_prefixed(&raw))
            .header(
                "x-idempotency-key",
                "65000000-0000-4000-8000-000000000002",
            )
            .header("idempotency-key", DELIVERY_ID)
            .body(Body::from(raw))
            .unwrap();
        let rejected = test_router(local).oneshot(mismatched).await.unwrap();
        assert_eq!(rejected.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn fingerprints_change_with_each_authority_component() {
        let base = delivery_request_fingerprint(
            DELIVERY_ID,
            &format!("sha256:{}", "1".repeat(64)),
            "$event",
            "@alice:example",
            "!room:example",
        );
        let variants = [
            delivery_request_fingerprint(
                "65000000-0000-4000-8000-000000000002",
                &format!("sha256:{}", "1".repeat(64)),
                "$event",
                "@alice:example",
                "!room:example",
            ),
            delivery_request_fingerprint(
                DELIVERY_ID,
                &format!("sha256:{}", "2".repeat(64)),
                "$event",
                "@alice:example",
                "!room:example",
            ),
            delivery_request_fingerprint(
                DELIVERY_ID,
                &format!("sha256:{}", "1".repeat(64)),
                "$other",
                "@alice:example",
                "!room:example",
            ),
            delivery_request_fingerprint(
                DELIVERY_ID,
                &format!("sha256:{}", "1".repeat(64)),
                "$event",
                "@bob:example",
                "!room:example",
            ),
            delivery_request_fingerprint(
                DELIVERY_ID,
                &format!("sha256:{}", "1".repeat(64)),
                "$event",
                "@alice:example",
                "!other:example",
            ),
        ];
        assert!(variants.iter().all(|value| value != &base));
    }
}
