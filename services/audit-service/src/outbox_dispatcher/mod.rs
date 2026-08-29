mod config;
mod delivery;
mod repository;

pub use config::AuditOutboxDispatcherConfig;
use config::retry_delay_seconds;
use delivery::deliver_item;
use repository::{acknowledge_delivery, claim_batch, record_delivery_failure};
use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub const DISPATCHER_SERVICE_ID: &str = "audit-outbox-dispatcher";

#[derive(Debug, Clone, Default)]
pub struct DispatchBatchStats {
    pub claimed: usize,
    pub delivered: usize,
    pub retry_scheduled: usize,
    pub dead_lettered: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ClaimedOutbox {
    pub(crate) outbox_id: Uuid,
    pub(crate) event_id: Uuid,
    pub(crate) source_service: String,
    pub(crate) attempt_count: i32,
    pub(crate) max_attempts: i32,
    pub(crate) envelope: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct VerifiedDelivery {
    pub(crate) http_status: i32,
    pub(crate) receipt: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct DeliveryFailure {
    pub(crate) retryable: bool,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) http_status: Option<u16>,
    pub(crate) retry_after_seconds: Option<i32>,
}

impl DeliveryFailure {
    pub(crate) fn retryable(
        code: &'static str,
        message: impl Into<String>,
        http_status: Option<u16>,
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

    pub(crate) fn permanent(
        code: &'static str,
        message: impl Into<String>,
        http_status: Option<u16>,
    ) -> Self {
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
    config: &AuditOutboxDispatcherConfig,
) -> Result<DispatchBatchStats, String> {
    let claimed = claim_batch(pool, config).await?;
    let mut stats = DispatchBatchStats {
        claimed: claimed.len(),
        ..DispatchBatchStats::default()
    };

    for item in claimed {
        match deliver_item(client, config, &item).await {
            Ok(delivery) => {
                // A verified remote append followed by a local ACK failure is not
                // converted into a remote failure. The row remains claimed and is
                // replayed after lease expiry with the same event id.
                acknowledge_delivery(pool, config, &item, &delivery).await?;
                stats.delivered += 1;
            }
            Err(mut failure) => {
                if failure.retryable {
                    failure.retry_after_seconds = Some(retry_delay_seconds(
                        item.event_id,
                        item.attempt_count,
                        failure.retry_after_seconds,
                    ));
                }

                let status = record_delivery_failure(pool, config, &item, &failure).await?;
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
