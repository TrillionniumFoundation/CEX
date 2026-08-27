use super::{
    config::{parse_retry_after_seconds, AuditOutboxDispatcherConfig},
    ClaimedOutbox, DeliveryFailure, VerifiedDelivery, DISPATCHER_SERVICE_ID,
};
use chrono::Utc;
use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use shared_types::audit_v2::{
    AuditEventCreateRequestV2, AuditEventRecordV2, AUDIT_WRITER_AUTH_SCHEME_V1,
};

#[derive(Debug, Deserialize)]
struct AuditAppendResponse {
    #[allow(dead_code)]
    replayed: bool,
    record: AuditEventRecordV2,
}

pub(crate) async fn deliver_item(
    client: &Client,
    config: &AuditOutboxDispatcherConfig,
    item: &ClaimedOutbox,
) -> Result<VerifiedDelivery, DeliveryFailure> {
    let request: AuditEventCreateRequestV2 =
        serde_json::from_value(item.envelope.clone()).map_err(|error| {
            DeliveryFailure::permanent(
                "invalid_outbox_envelope",
                format!("decode Audit v2 envelope: {error}"),
                None,
            )
        })?;

    request.validate(Utc::now()).map_err(|error| {
        DeliveryFailure::permanent(
            "invalid_outbox_contract",
            format!("validate Audit v2 envelope: {error}"),
            None,
        )
    })?;

    if request.event_id != item.event_id {
        return Err(DeliveryFailure::permanent(
            "event_id_mismatch",
            "outbox event_id differs from envelope event_id",
            None,
        ));
    }
    if request
        .payload
        .get("_cex_audit_source_service")
        .and_then(Value::as_str)
        != Some(item.source_service.as_str())
    {
        return Err(DeliveryFailure::permanent(
            "source_service_mismatch",
            "outbox source_service differs from payload source marker",
            None,
        ));
    }

    let response = client
        .post(config.endpoint())
        .timeout(config.request_timeout)
        .json(&request)
        .send()
        .await
        .map_err(|error| {
            DeliveryFailure::retryable(
                "audit_transport_error",
                format!("send Audit v2 request: {error}"),
                None,
                None,
            )
        })?;

    let status = response.status();
    let retry_after = parse_retry_after_seconds(response.headers().get(RETRY_AFTER));
    if let Some(length) = response.content_length() {
        if length > config.max_response_bytes as u64 {
            return Err(DeliveryFailure::retryable(
                "audit_response_too_large",
                format!(
                    "Audit v2 response Content-Length {length} exceeds {} bytes",
                    config.max_response_bytes
                ),
                Some(status.as_u16()),
                retry_after,
            ));
        }
    }

    let body = response.bytes().await.map_err(|error| {
        DeliveryFailure::retryable(
            "audit_response_read_error",
            format!("read Audit v2 response: {error}"),
            Some(status.as_u16()),
            retry_after,
        )
    })?;
    if body.len() > config.max_response_bytes {
        return Err(DeliveryFailure::retryable(
            "audit_response_too_large",
            format!(
                "Audit v2 response body {} exceeds {} bytes",
                body.len(),
                config.max_response_bytes
            ),
            Some(status.as_u16()),
            retry_after,
        ));
    }

    if status.is_success() {
        let append: AuditAppendResponse = serde_json::from_slice(&body).map_err(|error| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                format!("decode Audit v2 success receipt: {error}"),
                Some(status.as_u16()),
                retry_after,
            )
        })?;
        verify_success_receipt(&request, &append.record).map_err(|message| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                message,
                Some(status.as_u16()),
                retry_after,
            )
        })?;
        let receipt = serde_json::from_slice(&body).map_err(|error| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                format!("retain Audit v2 success receipt: {error}"),
                Some(status.as_u16()),
                retry_after,
            )
        })?;
        return Ok(VerifiedDelivery {
            http_status: i32::from(status.as_u16()),
            receipt,
        });
    }

    let message = response_message(&body);
    if is_retryable_status(status) {
        Err(DeliveryFailure::retryable(
            "audit_retryable_http",
            message,
            Some(status.as_u16()),
            retry_after,
        ))
    } else {
        Err(DeliveryFailure::permanent(
            if status == StatusCode::CONFLICT {
                "audit_event_collision"
            } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                "audit_auth_rejected"
            } else if status.is_redirection() {
                "audit_redirect_rejected"
            } else {
                "audit_permanent_http"
            },
            message,
            Some(status.as_u16()),
        ))
    }
}

fn verify_success_receipt(
    request: &AuditEventCreateRequestV2,
    record: &AuditEventRecordV2,
) -> Result<(), String> {
    if record.event_id != request.event_id
        || record.trace_id != request.trace_id
        || record.org_id != request.org_id
        || record.actor_type != request.actor_type
        || record.actor_id != request.actor_id
        || record.event_type != request.event_type
        || record.schema_version != request.schema_version
        || record.payload != request.payload
        || record.occurred_at.timestamp_micros() != request.occurred_at.timestamp_micros()
    {
        return Err("Audit v2 receipt immutable fields differ from the request".to_string());
    }
    if record.writer_service_id != DISPATCHER_SERVICE_ID
        || record.writer_auth_scheme != AUDIT_WRITER_AUTH_SCHEME_V1
    {
        return Err(
            "Audit v2 receipt writer identity is not the authenticated dispatcher".to_string(),
        );
    }
    let expected_chain_key = request
        .org_id
        .map(|org_id| format!("org:{org_id}"))
        .unwrap_or_else(|| "global".to_string());
    if record.chain_key != expected_chain_key {
        return Err("Audit v2 receipt chain key differs from the request tenancy".to_string());
    }
    if record.tenant_sequence <= 0 || !valid_event_hash(&record.event_hash) {
        return Err("Audit v2 receipt sequence or event hash is invalid".to_string());
    }
    if record
        .previous_event_hash
        .as_deref()
        .is_some_and(|value| !valid_event_hash(value))
    {
        return Err("Audit v2 receipt previous hash is invalid".to_string());
    }
    Ok(())
}

fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error()
        || matches!(
            status,
            StatusCode::REQUEST_TIMEOUT
                | StatusCode::TOO_EARLY
                | StatusCode::TOO_MANY_REQUESTS
        )
}

fn valid_event_hash(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn response_message(body: &[u8]) -> String {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if let Some(message) = value
            .get("message")
            .or_else(|| value.get("error"))
            .and_then(Value::as_str)
        {
            return truncate(message, 1000);
        }
    }
    truncate(&String::from_utf8_lossy(body), 1000)
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn retryable_statuses_are_narrowly_classified() {
        assert!(is_retryable_status(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(StatusCode::TOO_EARLY));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::CONFLICT));
    }

    #[test]
    fn event_hash_validation_is_strict() {
        let valid = format!("sha256:{}", "a".repeat(64));
        assert!(valid_event_hash(&valid));
        assert!(!valid_event_hash("sha256:abc"));
        assert!(!valid_event_hash(&format!("sha256:{}", "A".repeat(64))));
    }

    #[test]
    fn response_message_prefers_structured_error() {
        assert_eq!(
            response_message(&serde_json::to_vec(&json!({"error": "denied"})).unwrap()),
            "denied"
        );
    }
}
