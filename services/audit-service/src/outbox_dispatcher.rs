use chrono::Utc;
use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use shared_types::audit_v2::{
    AuditEventCreateRequestV2, AuditEventRecordV2, AUDIT_WRITER_AUTH_SCHEME_V1,
};
use sqlx::{PgPool, Row};
use std::{env, time::Duration};
use uuid::Uuid;

pub const DISPATCHER_SERVICE_ID: &str = "audit-outbox-dispatcher";

#[derive(Debug, Clone)]
pub struct DispatcherConfig {
    pub database_url: String,
    pub audit_base_url: String,
    pub worker_id: String,
    pub batch_size: i32,
    pub lease_seconds: i32,
    pub poll_interval: Duration,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
}

impl DispatcherConfig {
    pub fn from_env() -> Result<Self, String> {
        let database_url = required_env("DATABASE_URL")?;
        let audit_base_url = env::var("AUDIT_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:7004".to_string())
            .trim()
            .trim_end_matches('/')
            .to_string();
        if !(audit_base_url.starts_with("http://") || audit_base_url.starts_with("https://")) {
            return Err("AUDIT_BASE_URL must use http:// or https://".to_string());
        }

        let worker_id = env::var("CEX_AUDIT_OUTBOX_WORKER_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("{DISPATCHER_SERVICE_ID}-{}", Uuid::new_v4()));
        if worker_id.chars().count() > 128 {
            return Err("CEX_AUDIT_OUTBOX_WORKER_ID must not exceed 128 characters".to_string());
        }

        let batch_size = parse_i32_env("CEX_AUDIT_OUTBOX_BATCH_SIZE", 25, 1, 200)?;
        let lease_seconds = parse_i32_env("CEX_AUDIT_OUTBOX_LEASE_SECONDS", 60, 5, 600)?;
        let poll_seconds = parse_u64_env("CEX_AUDIT_OUTBOX_POLL_SECONDS", 2, 1, 60)?;
        let request_timeout_seconds =
            parse_u64_env("CEX_AUDIT_OUTBOX_REQUEST_TIMEOUT_SECONDS", 20, 1, 300)?;
        if request_timeout_seconds >= lease_seconds as u64 {
            return Err(
                "CEX_AUDIT_OUTBOX_REQUEST_TIMEOUT_SECONDS must be shorter than the claim lease"
                    .to_string(),
            );
        }
        let max_response_bytes = parse_usize_env(
            "CEX_AUDIT_OUTBOX_MAX_RESPONSE_BYTES",
            262_144,
            1_024,
            4_194_304,
        )?;

        Ok(Self {
            database_url,
            audit_base_url,
            worker_id,
            batch_size,
            lease_seconds,
            poll_interval: Duration::from_secs(poll_seconds),
            request_timeout: Duration::from_secs(request_timeout_seconds),
            max_response_bytes,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/v2/audit/events", self.audit_base_url)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatchStats {
    pub claimed: usize,
    pub delivered: usize,
    pub retry_scheduled: usize,
    pub dead_lettered: usize,
}

#[derive(Debug, Clone)]
struct ClaimedOutbox {
    outbox_id: Uuid,
    event_id: Uuid,
    source_service: String,
    attempt_count: i32,
    max_attempts: i32,
    envelope: Value,
}

#[derive(Debug, Deserialize)]
struct AppendResponse {
    #[allow(dead_code)]
    replayed: bool,
    record: AuditEventRecordV2,
}

#[derive(Debug, Clone)]
struct VerifiedDelivery {
    http_status: i32,
    receipt: Value,
}

#[derive(Debug, Clone)]
struct DeliveryFailure {
    retryable: bool,
    code: &'static str,
    message: String,
    http_status: Option<i32>,
    retry_after_seconds: Option<i32>,
}

impl DeliveryFailure {
    fn retryable(
        code: &'static str,
        message: impl Into<String>,
        http_status: Option<i32>,
        retry_after_seconds: Option<i32>,
    ) -> Self {
        Self {
            retryable: true,
            code,
            message: message.into(),
            http_status,
            retry_after_seconds,
        }
    }

    fn permanent(code: &'static str, message: impl Into<String>, http_status: Option<i32>) -> Self {
        Self {
            retryable: false,
            code,
            message: message.into(),
            http_status,
            retry_after_seconds: None,
        }
    }
}

pub async fn dispatch_once(
    pool: &PgPool,
    client: &Client,
    config: &DispatcherConfig,
) -> Result<BatchStats, String> {
    let claimed = claim_batch(pool, config).await?;
    let mut stats = BatchStats {
        claimed: claimed.len(),
        ..BatchStats::default()
    };

    for item in claimed {
        match deliver_item(client, config, &item).await {
            Ok(delivery) => {
                acknowledge_delivery(pool, config, &item, &delivery).await?;
                stats.delivered += 1;
            }
            Err(mut failure) => {
                if failure.retryable && item.attempt_count < item.max_attempts {
                    failure.retry_after_seconds = Some(retry_delay_seconds(
                        item.event_id,
                        item.attempt_count,
                        failure.retry_after_seconds,
                    ));
                }

                let status = record_failure(pool, config, &item, &failure).await?;
                match status.as_str() {
                    "retry_wait" => stats.retry_scheduled += 1,
                    "dead_letter" => stats.dead_lettered += 1,
                    other => {
                        return Err(format!(
                            "unexpected audit outbox failure transition status '{other}'"
                        ))
                    }
                }
            }
        }
    }

    Ok(stats)
}

async fn claim_batch(
    pool: &PgPool,
    config: &DispatcherConfig,
) -> Result<Vec<ClaimedOutbox>, String> {
    let rows = sqlx::query(
        r#"
select outbox_id, event_id, source_service, attempt_count, max_attempts, envelope
from public.cex_claim_audit_outbox_v1($1, $2, $3)
        "#,
    )
    .bind(&config.worker_id)
    .bind(config.batch_size)
    .bind(config.lease_seconds)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("claim audit outbox batch: {error}"))?;

    rows.into_iter()
        .map(|row| {
            Ok(ClaimedOutbox {
                outbox_id: row
                    .try_get("outbox_id")
                    .map_err(|error| format!("decode outbox_id: {error}"))?,
                event_id: row
                    .try_get("event_id")
                    .map_err(|error| format!("decode event_id: {error}"))?,
                source_service: row
                    .try_get("source_service")
                    .map_err(|error| format!("decode source_service: {error}"))?,
                attempt_count: row
                    .try_get("attempt_count")
                    .map_err(|error| format!("decode attempt_count: {error}"))?,
                max_attempts: row
                    .try_get("max_attempts")
                    .map_err(|error| format!("decode max_attempts: {error}"))?,
                envelope: row
                    .try_get("envelope")
                    .map_err(|error| format!("decode envelope: {error}"))?,
            })
        })
        .collect()
}

async fn deliver_item(
    client: &Client,
    config: &DispatcherConfig,
    item: &ClaimedOutbox,
) -> Result<VerifiedDelivery, DeliveryFailure> {
    let request: AuditEventCreateRequestV2 = serde_json::from_value(item.envelope.clone())
        .map_err(|error| {
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
    let status_code = i32::from(status.as_u16());
    let retry_after = parse_retry_after_seconds(response.headers().get(RETRY_AFTER));
    if let Some(length) = response.content_length() {
        if length > config.max_response_bytes as u64 {
            return Err(classified_failure(
                status,
                "audit_response_too_large",
                format!(
                    "Audit v2 response Content-Length {length} exceeds {} bytes",
                    config.max_response_bytes
                ),
                retry_after,
            ));
        }
    }

    let body = response.bytes().await.map_err(|error| {
        DeliveryFailure::retryable(
            "audit_response_read_error",
            format!("read Audit v2 response: {error}"),
            Some(status_code),
            retry_after,
        )
    })?;
    if body.len() > config.max_response_bytes {
        return Err(classified_failure(
            status,
            "audit_response_too_large",
            format!(
                "Audit v2 response body {} exceeds {} bytes",
                body.len(),
                config.max_response_bytes
            ),
            retry_after,
        ));
    }

    if status.is_success() {
        let append: AppendResponse = serde_json::from_slice(&body).map_err(|error| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                format!("decode Audit v2 success receipt: {error}"),
                Some(status_code),
                retry_after,
            )
        })?;
        verify_success_receipt(&request, &append.record).map_err(|message| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                message,
                Some(status_code),
                retry_after,
            )
        })?;
        let receipt = serde_json::from_slice(&body).map_err(|error| {
            DeliveryFailure::retryable(
                "invalid_success_receipt",
                format!("retain Audit v2 success receipt: {error}"),
                Some(status_code),
                retry_after,
            )
        })?;
        return Ok(VerifiedDelivery {
            http_status: status_code,
            receipt,
        });
    }

    Err(classified_failure(
        status,
        if status == StatusCode::CONFLICT {
            "audit_event_collision"
        } else if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            "audit_auth_rejected"
        } else if status.is_redirection() {
            "audit_redirect_rejected"
        } else if is_retryable_status(status) {
            "audit_retryable_http"
        } else {
            "audit_permanent_http"
        },
        response_message(&body),
        retry_after,
    ))
}

fn classified_failure(
    status: StatusCode,
    code: &'static str,
    message: impl Into<String>,
    retry_after_seconds: Option<i32>,
) -> DeliveryFailure {
    let status_code = Some(i32::from(status.as_u16()));
    if status.is_success() || is_retryable_status(status) {
        DeliveryFailure::retryable(code, message, status_code, retry_after_seconds)
    } else {
        DeliveryFailure::permanent(code, message, status_code)
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
        return Err("Audit v2 receipt writer identity is not the dispatcher".to_string());
    }
    let expected_chain_key = request
        .org_id
        .map(|org_id| format!("org:{org_id}"))
        .unwrap_or_else(|| "global".to_string());
    if record.chain_key != expected_chain_key {
        return Err("Audit v2 receipt chain key differs from request tenancy".to_string());
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

async fn acknowledge_delivery(
    pool: &PgPool,
    config: &DispatcherConfig,
    item: &ClaimedOutbox,
    delivery: &VerifiedDelivery,
) -> Result<(), String> {
    let receipt = &delivery.receipt;
    let event_hash = receipt
        .pointer("/record/event_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| "receipt missing record.event_hash".to_string())?;
    let tenant_sequence = receipt
        .pointer("/record/tenant_sequence")
        .and_then(Value::as_i64)
        .ok_or_else(|| "receipt missing record.tenant_sequence".to_string())?;
    let event_id = receipt
        .pointer("/record/event_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "receipt missing record.event_id".to_string())
        .and_then(|value| {
            Uuid::parse_str(value).map_err(|error| format!("decode receipt event_id: {error}"))
        })?;

    let status = sqlx::query_scalar::<_, String>(
        r#"
select status
from public.cex_mark_audit_outbox_delivered_v1($1, $2, $3, $4, $5, $6::jsonb, $7)
        "#,
    )
    .bind(item.outbox_id)
    .bind(&config.worker_id)
    .bind(event_id)
    .bind(event_hash)
    .bind(tenant_sequence)
    .bind(receipt)
    .bind(delivery.http_status)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("acknowledge audit outbox delivery: {error}"))?;

    if status != "delivered" {
        return Err(format!("audit outbox ACK returned status '{status}'"));
    }
    Ok(())
}

async fn record_failure(
    pool: &PgPool,
    config: &DispatcherConfig,
    item: &ClaimedOutbox,
    failure: &DeliveryFailure,
) -> Result<String, String> {
    let retryable = failure.retryable && item.attempt_count < item.max_attempts;
    sqlx::query_scalar::<_, String>(
        r#"
select status
from public.cex_fail_audit_outbox_delivery_v1($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(item.outbox_id)
    .bind(&config.worker_id)
    .bind(retryable)
    .bind(failure.code)
    .bind(&failure.message)
    .bind(failure.http_status)
    .bind(if retryable {
        failure.retry_after_seconds
    } else {
        None
    })
    .fetch_one(pool)
    .await
    .map_err(|error| format!("record audit outbox delivery failure: {error}"))
}

fn is_retryable_status(status: StatusCode) -> bool {
    status.is_server_error()
        || matches!(
            status,
            StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_EARLY | StatusCode::TOO_MANY_REQUESTS
        )
}

fn retry_delay_seconds(
    event_id: Uuid,
    attempt_count: i32,
    retry_after_seconds: Option<i32>,
) -> i32 {
    let exponent = attempt_count.saturating_sub(1).clamp(0, 10) as u32;
    let base = 1_i32.checked_shl(exponent).unwrap_or(1_024).clamp(1, 3_600);
    let jitter_bound = (base / 4).max(1);
    let jitter = i32::from(event_id.as_bytes()[0]) % (jitter_bound + 1);
    (base + jitter)
        .max(retry_after_seconds.unwrap_or_default())
        .clamp(1, 3_600)
}

fn parse_retry_after_seconds(value: Option<&reqwest::header::HeaderValue>) -> Option<i32> {
    value
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|seconds| (1..=3_600).contains(seconds))
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
            return truncate(message, 1_000);
        }
    }
    truncate(&String::from_utf8_lossy(body), 1_000)
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn parse_i32_env(name: &str, default: i32, min: i32, max: i32) -> Result<i32, String> {
    let value = env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<i32>()
                .map_err(|error| format!("parse {name}: {error}"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

fn parse_u64_env(name: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
    let value = env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| format!("parse {name}: {error}"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

fn parse_usize_env(name: &str, default: usize, min: usize, max: usize) -> Result<usize, String> {
    let value = env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|error| format!("parse {name}: {error}"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
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
    fn retry_delay_is_bounded_and_deterministic() {
        let event_id = Uuid::parse_str("10000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(
            retry_delay_seconds(event_id, 1, None),
            retry_delay_seconds(event_id, 1, None)
        );
        assert!((1..=3_600).contains(&retry_delay_seconds(event_id, 20, None)));
        assert_eq!(retry_delay_seconds(event_id, 1, Some(300)), 300);
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
