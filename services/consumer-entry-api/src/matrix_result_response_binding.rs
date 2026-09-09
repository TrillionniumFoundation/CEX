use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header::CONTENT_LENGTH, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const LOOKUP_PATH: &str = "/v1/matrix/messages/result";
const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const DELIVERY_BINDING_FIELD: &str = "cex_delivery_binding";
const DELIVERY_BINDING_SCHEMA: &str = "cex.matrix.delivery-binding.v1";
const DELIVERY_BINDING_SOURCE: &str = "matrix-bot-relay-headers-v1";
const DELIVERY_FINGERPRINT_DOMAIN: &str = "cex.matrix.adapter-result-delivery.v1";

#[derive(Debug)]
struct ExpectedBinding {
    delivery_id: String,
    event_id: String,
    room_id: String,
    matrix_user_id: String,
    payload_sha256: String,
    request_fingerprint: String,
}

pub async fn enforce_matrix_result_response_binding(
    request: Request,
    next: Next,
) -> Response {
    if request.method() != Method::POST || request.uri().path() != LOOKUP_PATH {
        return next.run(request).await;
    }

    let (mut request_parts, request_body) = request.into_parts();
    let request_bytes = match to_bytes(request_body, MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return lookup_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "matrix_result_lookup_request_unavailable",
            )
        }
    };
    // The authenticated inner handler remains authoritative for request errors.
    // This parsed expectation is enforced only if an inner layer emits success.
    let expected = parse_expected_binding(&request_bytes);
    request_parts.headers.remove(CONTENT_LENGTH);
    let response = next
        .run(Request::from_parts(
            request_parts,
            Body::from(request_bytes),
        ))
        .await;
    if !response.status().is_success() {
        return response;
    }
    let expected = match expected {
        Ok(expected) => expected,
        Err(_) => {
            return lookup_error(
                StatusCode::CONFLICT,
                "matrix_result_lookup_success_for_invalid_request",
            )
        }
    };

    let (mut response_parts, response_body) = response.into_parts();
    let response_bytes = match to_bytes(response_body, MAX_RESPONSE_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return lookup_error(
                StatusCode::BAD_GATEWAY,
                "matrix_result_lookup_response_unavailable",
            )
        }
    };
    if validate_success_response(&response_bytes, &expected).is_err() {
        return lookup_error(
            StatusCode::CONFLICT,
            "matrix_result_lookup_task_or_delivery_binding_mismatch",
        );
    }

    response_parts.headers.remove(CONTENT_LENGTH);
    Response::from_parts(response_parts, Body::from(response_bytes))
}

fn parse_expected_binding(raw: &[u8]) -> Result<ExpectedBinding, ()> {
    let value: Value = serde_json::from_slice(raw).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    let expected = ExpectedBinding {
        delivery_id: required_text(object, "delivery_id")?.to_string(),
        event_id: required_text(object, "event_id")?.to_string(),
        room_id: required_text(object, "room_id")?.to_string(),
        matrix_user_id: required_text(object, "matrix_user_id")?.to_string(),
        payload_sha256: required_text(object, "payload_sha256")?.to_string(),
        request_fingerprint: required_text(object, "request_fingerprint")?.to_string(),
    };
    if !valid_delivery_id(&expected.delivery_id)
        || !valid_identifier(&expected.event_id, '$')
        || !valid_identifier(&expected.room_id, '!')
        || !valid_identifier(&expected.matrix_user_id, '@')
        || !valid_sha256(&expected.payload_sha256)
        || !valid_sha256(&expected.request_fingerprint)
        || expected.request_fingerprint != delivery_request_fingerprint(&expected)
    {
        return Err(());
    }
    Ok(expected)
}

fn validate_success_response(raw: &[u8], expected: &ExpectedBinding) -> Result<(), ()> {
    let value: Value = serde_json::from_slice(raw).map_err(|_| ())?;
    if value.get("schema").and_then(Value::as_str) != Some("cex.matrix.result-lookup.v1")
        || value.get("resolved").and_then(Value::as_bool) != Some(true)
        || value.get("delivery_id").and_then(Value::as_str)
            != Some(expected.delivery_id.as_str())
        || value.get("event_id").and_then(Value::as_str) != Some(expected.event_id.as_str())
        || value.get("matrix_user_id").and_then(Value::as_str)
            != Some(expected.matrix_user_id.as_str())
        || value.get("room_id").and_then(Value::as_str) != Some(expected.room_id.as_str())
        || value.get("payload_sha256").and_then(Value::as_str)
            != Some(expected.payload_sha256.as_str())
        || value.get("request_fingerprint").and_then(Value::as_str)
            != Some(expected.request_fingerprint.as_str())
        || value.get("seen_at_epoch").and_then(Value::as_i64).is_none()
        || value.get("production_authorization").and_then(Value::as_str)
            != Some("not_granted")
    {
        return Err(());
    }

    let top_binding = value
        .get("result_delivery_binding")
        .and_then(Value::as_object)
        .ok_or(())?;
    validate_embedded_binding(top_binding, expected)?;

    let response = value.get("response").and_then(Value::as_object).ok_or(())?;
    let task_id = response
        .get("task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or(())?;
    let invocation_id = response
        .get("raw")
        .and_then(Value::as_object)
        .and_then(|raw| raw.get("invocation_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or(())?;
    if invocation_id != task_id {
        return Err(());
    }

    let source = response.get("source").and_then(Value::as_object).ok_or(())?;
    if source.get("kind").and_then(Value::as_str) != Some("matrix_message")
        || source.get("event_id").and_then(Value::as_str)
            != Some(expected.event_id.as_str())
        || source.get("room_id").and_then(Value::as_str)
            != Some(expected.room_id.as_str())
        || source.get("matrix_user_id").and_then(Value::as_str)
            != Some(expected.matrix_user_id.as_str())
    {
        return Err(());
    }
    let identity_scope = source
        .get("identity_scope")
        .and_then(Value::as_object)
        .ok_or(())?;
    if identity_scope.get("user_id").and_then(Value::as_str)
        != Some(expected.matrix_user_id.as_str())
        || identity_scope.get("room_id").and_then(Value::as_str)
            != Some(expected.room_id.as_str())
    {
        return Err(());
    }
    let nested_binding = source
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get("metadata"))
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(DELIVERY_BINDING_FIELD))
        .and_then(Value::as_object)
        .ok_or(())?;
    validate_embedded_binding(nested_binding, expected)?;
    if nested_binding != top_binding {
        return Err(());
    }
    Ok(())
}

fn validate_embedded_binding(
    binding: &Map<String, Value>,
    expected: &ExpectedBinding,
) -> Result<(), ()> {
    if binding.len() != 8
        || binding.get("schema").and_then(Value::as_str) != Some(DELIVERY_BINDING_SCHEMA)
        || binding.get("source").and_then(Value::as_str) != Some(DELIVERY_BINDING_SOURCE)
        || binding.get("delivery_id").and_then(Value::as_str)
            != Some(expected.delivery_id.as_str())
        || binding.get("payload_sha256").and_then(Value::as_str)
            != Some(expected.payload_sha256.as_str())
        || binding.get("event_id").and_then(Value::as_str)
            != Some(expected.event_id.as_str())
        || binding.get("room_id").and_then(Value::as_str)
            != Some(expected.room_id.as_str())
        || binding.get("matrix_user_id").and_then(Value::as_str)
            != Some(expected.matrix_user_id.as_str())
        || binding.get("request_fingerprint").and_then(Value::as_str)
            != Some(expected.request_fingerprint.as_str())
        || expected.request_fingerprint != delivery_request_fingerprint(expected)
    {
        return Err(());
    }
    Ok(())
}

fn required_text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, ()> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value == value.trim())
        .ok_or(())
}

fn delivery_request_fingerprint(expected: &ExpectedBinding) -> String {
    let mut hasher = Sha256::new();
    for value in [
        DELIVERY_FINGERPRINT_DOMAIN,
        expected.delivery_id.as_str(),
        expected.payload_sha256.as_str(),
        expected.event_id.as_str(),
        expected.matrix_user_id.as_str(),
        expected.room_id.as_str(),
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

fn valid_identifier(value: &str, prefix: char) -> bool {
    value.len() >= 2
        && value.len() <= 512
        && value.starts_with(prefix)
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}

fn lookup_error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(json!({
            "schema": "cex.matrix.result-lookup.v1",
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
    use axum::{routing::post, Router};
    use tower::ServiceExt;

    fn expected() -> ExpectedBinding {
        let mut expected = ExpectedBinding {
            delivery_id: "65000000-0000-4000-8000-000000000001".to_string(),
            event_id: "$delivery-bound-event".to_string(),
            room_id: "!delivery-room:example".to_string(),
            matrix_user_id: "@delivery-user:example".to_string(),
            payload_sha256: format!("sha256:{}", "8".repeat(64)),
            request_fingerprint: String::new(),
        };
        expected.request_fingerprint = delivery_request_fingerprint(&expected);
        expected
    }

    fn request(expected: &ExpectedBinding) -> Request {
        Request::builder()
            .method(Method::POST)
            .uri(LOOKUP_PATH)
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({
                    "delivery_id": expected.delivery_id,
                    "event_id": expected.event_id,
                    "matrix_user_id": expected.matrix_user_id,
                    "room_id": expected.room_id,
                    "payload_sha256": expected.payload_sha256,
                    "request_fingerprint": expected.request_fingerprint,
                }))
                .unwrap(),
            ))
            .unwrap()
    }

    fn binding(expected: &ExpectedBinding) -> Value {
        json!({
            "schema": DELIVERY_BINDING_SCHEMA,
            "source": DELIVERY_BINDING_SOURCE,
            "delivery_id": expected.delivery_id,
            "payload_sha256": expected.payload_sha256,
            "event_id": expected.event_id,
            "room_id": expected.room_id,
            "matrix_user_id": expected.matrix_user_id,
            "request_fingerprint": expected.request_fingerprint,
        })
    }

    fn success(expected: &ExpectedBinding) -> Value {
        let binding = binding(expected);
        json!({
            "schema": "cex.matrix.result-lookup.v1",
            "resolved": true,
            "delivery_id": expected.delivery_id,
            "event_id": expected.event_id,
            "matrix_user_id": expected.matrix_user_id,
            "room_id": expected.room_id,
            "payload_sha256": expected.payload_sha256,
            "request_fingerprint": expected.request_fingerprint,
            "result_delivery_binding": binding,
            "seen_at_epoch": 1_789_000_000_i64,
            "response": {
                "task_id": "task-delivery-bound",
                "source": {
                    "kind": "matrix_message",
                    "event_id": expected.event_id,
                    "room_id": expected.room_id,
                    "matrix_user_id": expected.matrix_user_id,
                    "identity_scope": {
                        "user_id": expected.matrix_user_id,
                        "room_id": expected.room_id,
                    },
                    "metadata": {"metadata": {DELIVERY_BINDING_FIELD: binding(expected)}}
                },
                "raw": {"invocation_id": "task-delivery-bound"}
            },
            "production_authorization": "not_granted"
        })
    }

    fn router(response: Value) -> Router {
        Router::new()
            .route(
                LOOKUP_PATH,
                post(move || {
                    let response = response.clone();
                    async move { Json(response) }
                }),
            )
            .layer(axum::middleware::from_fn(
                enforce_matrix_result_response_binding,
            ))
    }

    #[tokio::test]
    async fn exact_task_and_delivery_binding_pass() {
        let expected = expected();
        let response = router(success(&expected))
            .oneshot(request(&expected))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_or_changed_invocation_fails_closed() {
        let expected = expected();
        let mut missing = success(&expected);
        missing["response"]["raw"] = json!({});
        let response = router(missing)
            .oneshot(request(&expected))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let mut changed = success(&expected);
        changed["response"]["raw"]["invocation_id"] = json!("other-task");
        let response = router(changed)
            .oneshot(request(&expected))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn changed_top_or_nested_delivery_binding_fails_closed() {
        let expected = expected();
        let mut top = success(&expected);
        top["result_delivery_binding"]["payload_sha256"] =
            json!(format!("sha256:{}", "9".repeat(64)));
        let response = router(top)
            .oneshot(request(&expected))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let mut nested = success(&expected);
        nested["response"]["source"]["metadata"]["metadata"]
            [DELIVERY_BINDING_FIELD]["forged"] = json!(true);
        let response = router(nested)
            .oneshot(request(&expected))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
}