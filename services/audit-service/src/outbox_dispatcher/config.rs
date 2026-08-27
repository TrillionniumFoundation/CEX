use super::DISPATCHER_SERVICE_ID;
use chrono::{DateTime, Utc};
use reqwest::header::HeaderValue;
use std::{env, time::Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditOutboxDispatcherConfig {
    pub database_url: String,
    pub audit_base_url: String,
    pub worker_id: String,
    pub batch_size: i32,
    pub lease_seconds: i32,
    pub poll_interval: Duration,
    pub request_timeout: Duration,
    pub max_response_bytes: usize,
}

impl AuditOutboxDispatcherConfig {
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

        let batch_size = parse_i32_env("CEX_AUDIT_OUTBOX_BATCH_SIZE", 10, 1, 200)?;
        let lease_seconds = parse_i32_env("CEX_AUDIT_OUTBOX_LEASE_SECONDS", 300, 5, 600)?;
        let poll_seconds = parse_u64_env("CEX_AUDIT_OUTBOX_POLL_SECONDS", 2, 1, 60)?;
        let request_timeout_seconds =
            parse_u64_env("CEX_AUDIT_OUTBOX_REQUEST_TIMEOUT_SECONDS", 20, 1, 300)?;
        let worst_case_batch_seconds = request_timeout_seconds
            .saturating_mul(batch_size as u64)
            .saturating_add(5);
        if worst_case_batch_seconds >= lease_seconds as u64 {
            return Err(format!(
                "sequential dispatcher safety requires lease_seconds > batch_size * request_timeout_seconds + 5 (computed {worst_case_batch_seconds})"
            ));
        }
        let max_response_bytes =
            parse_usize_env("CEX_AUDIT_OUTBOX_MAX_RESPONSE_BYTES", 262_144, 1024, 4_194_304)?;

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

    pub fn endpoint(&self) -> String {
        format!("{}/v2/audit/events", self.audit_base_url)
    }
}

pub(crate) fn retry_delay_seconds(
    event_id: Uuid,
    attempt_count: i32,
    retry_after_seconds: Option<i32>,
) -> i32 {
    let exponent = (attempt_count.saturating_sub(1)).clamp(0, 10) as u32;
    let base = 1_i32.checked_shl(exponent).unwrap_or(1024).clamp(1, 3600);
    let jitter_bound = (base / 4).max(1);
    let jitter = i32::from(event_id.as_bytes()[0]) % (jitter_bound + 1);
    (base + jitter)
        .max(retry_after_seconds.unwrap_or_default())
        .clamp(1, 3600)
}

pub(crate) fn parse_retry_after_seconds(value: Option<&HeaderValue>) -> Option<i32> {
    let value = value.and_then(|value| value.to_str().ok())?.trim();
    if let Ok(seconds) = value.parse::<i32>() {
        return (1..=3600).contains(&seconds).then_some(seconds);
    }

    let retry_at = DateTime::parse_from_rfc2822(value).ok()?.with_timezone(&Utc);
    let seconds = retry_at.signed_duration_since(Utc::now()).num_seconds();
    (1..=3600)
        .contains(&seconds)
        .then_some(seconds as i32)
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

fn parse_usize_env(
    name: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, String> {
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

    #[test]
    fn retry_delay_is_bounded_and_deterministic() {
        let event_id = Uuid::parse_str("10000000-0000-4000-8000-000000000001").unwrap();
        assert_eq!(
            retry_delay_seconds(event_id, 1, None),
            retry_delay_seconds(event_id, 1, None)
        );
        assert!((1..=3600).contains(&retry_delay_seconds(event_id, 20, None)));
        assert_eq!(retry_delay_seconds(event_id, 1, Some(300)), 300);
    }
}
