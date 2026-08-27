use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::Row;
use std::env;

use crate::infrastructure::state::{AppState, GatewayRuntimeMetricsSnapshot};

#[derive(Debug, Clone, Serialize)]
pub struct SagaQueueItem {
    pub execution_mode: String,
    pub command_kind: String,
    pub status: String,
    pub command_count: i64,
    pub oldest_available_at: Option<DateTime<Utc>>,
    pub oldest_lease_expiry: Option<DateTime<Utc>>,
    pub retry_budget_exhausted: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SagaReconciliationSummary {
    pub shadow_commands_total: i64,
    pub orphaned_workflows: i64,
    pub observed_status_mismatches: i64,
    pub observed_execution_id_mismatches: i64,
    pub reserve_observed_without_receipt: i64,
    pub invocations_missing_execution_shadow: i64,
    pub oldest_shadow_age_seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SagaOperationalSummary {
    pub service: &'static str,
    pub shadow_write_enabled: bool,
    pub database_configured: bool,
    pub runtime: GatewayRuntimeMetricsSnapshot,
    pub queue: Vec<SagaQueueItem>,
    pub reconciliation: SagaReconciliationSummary,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/saga/summary", get(summary))
        .route("/metrics/saga", get(metrics))
        .with_state(state)
}

async fn summary(State(state): State<AppState>) -> impl IntoResponse {
    match load_operational_summary(&state).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(error) => {
            eprintln!("gateway-service: saga summary unavailable: {error}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "saga operational summary unavailable" })),
            )
                .into_response()
        }
    }
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = match load_operational_summary(&state).await {
        Ok(summary) => render_metrics(&summary),
        Err(error) => {
            eprintln!("gateway-service: saga metrics unavailable: {error}");
            concat!(
                "# HELP cex_gateway_saga_runtime_up Whether saga tables and reconciliation queries are available.\n",
                "# TYPE cex_gateway_saga_runtime_up gauge\n",
                "cex_gateway_saga_runtime_up 0\n",
            )
            .to_string()
        }
    };

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn load_operational_summary(state: &AppState) -> Result<SagaOperationalSummary, String> {
    let pool = state
        .pool
        .as_ref()
        .ok_or_else(|| "gateway postgres pool not initialized".to_string())?;

    let queue_rows = sqlx::query(
        "select
            execution_mode,
            command_kind,
            status,
            command_count,
            oldest_available_at,
            oldest_lease_expiry,
            retry_budget_exhausted
         from cex_saga_queue_summary_v1
         order by execution_mode, command_kind, status",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("load saga queue summary: {error}"))?;

    let mut queue = Vec::with_capacity(queue_rows.len());
    for row in queue_rows {
        queue.push(SagaQueueItem {
            execution_mode: row
                .try_get("execution_mode")
                .map_err(|error| format!("read saga execution_mode: {error}"))?,
            command_kind: row
                .try_get("command_kind")
                .map_err(|error| format!("read saga command_kind: {error}"))?,
            status: row
                .try_get("status")
                .map_err(|error| format!("read saga status: {error}"))?,
            command_count: row
                .try_get("command_count")
                .map_err(|error| format!("read saga command_count: {error}"))?,
            oldest_available_at: row
                .try_get("oldest_available_at")
                .map_err(|error| format!("read saga oldest_available_at: {error}"))?,
            oldest_lease_expiry: row
                .try_get("oldest_lease_expiry")
                .map_err(|error| format!("read saga oldest_lease_expiry: {error}"))?,
            retry_budget_exhausted: row
                .try_get("retry_budget_exhausted")
                .map_err(|error| format!("read saga retry_budget_exhausted: {error}"))?,
        });
    }

    Ok(SagaOperationalSummary {
        service: "gateway-service",
        shadow_write_enabled: shadow_write_enabled(),
        database_configured: true,
        runtime: state.metrics.snapshot(),
        queue,
        reconciliation: load_reconciliation(pool).await?,
    })
}

async fn load_reconciliation(pool: &sqlx::PgPool) -> Result<SagaReconciliationSummary, String> {
    let row = sqlx::query(
        "select
            count(*)::bigint as shadow_commands_total,
            count(*) filter (
                where saga.workflow_kind = 'invocation'
                  and invocation.invocation_id is null
            )::bigint as orphaned_workflows,
            count(*) filter (
                where saga.workflow_kind = 'invocation'
                  and invocation.invocation_id is not null
                  and saga.payload #>> '{observed,status}'
                      is distinct from invocation.status
            )::bigint as observed_status_mismatches,
            count(*) filter (
                where saga.command_kind = 'execution_create'
                  and invocation.invocation_id is not null
                  and coalesce(saga.payload #>> '{observed,execution_id}', '')
                      <> coalesce(invocation.execution_id::text, '')
            )::bigint as observed_execution_id_mismatches,
            count(*) filter (
                where saga.command_kind = 'ledger_reserve'
                  and saga.payload #>> '{observed,ledger_reserved}' = 'true'
                  and not exists (
                      select 1
                        from ledger_entries entry
                       where entry.idempotency_key = saga.payload ->> 'legacy_idempotency_key'
                  )
            )::bigint as reserve_observed_without_receipt,
            coalesce(
                extract(epoch from (now() - min(saga.created_at)))::bigint,
                0
            ) as oldest_shadow_age_seconds
         from cex_saga_commands_v1 saga
         left join invocations invocation
           on saga.workflow_kind = 'invocation'
          and invocation.invocation_id = saga.workflow_id
         where saga.execution_mode = 'shadow'",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("load saga reconciliation aggregates: {error}"))?;

    let coverage_row = sqlx::query(
        "with boundary as (
            select min(created_at) as first_shadow_created_at
              from cex_saga_commands_v1
             where execution_mode = 'shadow'
               and workflow_kind = 'invocation'
         )
         select count(*)::bigint as missing
           from invocations invocation
           cross join boundary
          where boundary.first_shadow_created_at is not null
            and invocation.created_at >= boundary.first_shadow_created_at
            and not exists (
                select 1
                  from cex_saga_commands_v1 saga
                 where saga.execution_mode = 'shadow'
                   and saga.workflow_kind = 'invocation'
                   and saga.workflow_id = invocation.invocation_id
                   and saga.command_kind = 'execution_create'
            )",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("load saga shadow coverage: {error}"))?;

    Ok(SagaReconciliationSummary {
        shadow_commands_total: row
            .try_get("shadow_commands_total")
            .map_err(|error| format!("read shadow_commands_total: {error}"))?,
        orphaned_workflows: row
            .try_get("orphaned_workflows")
            .map_err(|error| format!("read orphaned_workflows: {error}"))?,
        observed_status_mismatches: row
            .try_get("observed_status_mismatches")
            .map_err(|error| format!("read observed_status_mismatches: {error}"))?,
        observed_execution_id_mismatches: row
            .try_get("observed_execution_id_mismatches")
            .map_err(|error| format!("read observed_execution_id_mismatches: {error}"))?,
        reserve_observed_without_receipt: row
            .try_get("reserve_observed_without_receipt")
            .map_err(|error| format!("read reserve_observed_without_receipt: {error}"))?,
        invocations_missing_execution_shadow: coverage_row
            .try_get("missing")
            .map_err(|error| format!("read missing execution shadow coverage: {error}"))?,
        oldest_shadow_age_seconds: row
            .try_get("oldest_shadow_age_seconds")
            .map_err(|error| format!("read oldest_shadow_age_seconds: {error}"))?,
    })
}

fn shadow_write_enabled() -> bool {
    env::var("CEX_SAGA_SHADOW_WRITE")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn render_metrics(summary: &SagaOperationalSummary) -> String {
    let mut body = concat!(
        "# HELP cex_gateway_saga_runtime_up Whether saga tables and reconciliation queries are available.\n",
        "# TYPE cex_gateway_saga_runtime_up gauge\n",
        "cex_gateway_saga_runtime_up 1\n",
        "# HELP cex_gateway_saga_shadow_write_total Gateway saga shadow-write outcomes.\n",
        "# TYPE cex_gateway_saga_shadow_write_total counter\n",
        "# HELP cex_gateway_saga_queue_commands Saga command count by mode, kind and status.\n",
        "# TYPE cex_gateway_saga_queue_commands gauge\n",
        "# HELP cex_gateway_saga_queue_retry_budget_exhausted Saga commands at attempt budget.\n",
        "# TYPE cex_gateway_saga_queue_retry_budget_exhausted gauge\n",
        "# HELP cex_gateway_saga_reconciliation_mismatches Saga shadow reconciliation mismatch counts.\n",
        "# TYPE cex_gateway_saga_reconciliation_mismatches gauge\n",
        "# HELP cex_gateway_saga_oldest_shadow_age_seconds Age of the oldest shadow command.\n",
        "# TYPE cex_gateway_saga_oldest_shadow_age_seconds gauge\n",
    )
    .to_string();

    for (result, value) in [
        ("attempt", summary.runtime.saga_shadow_write_attempts),
        ("success", summary.runtime.saga_shadow_write_successes),
        ("failure", summary.runtime.saga_shadow_write_failures),
    ] {
        body.push_str(&format!(
            "cex_gateway_saga_shadow_write_total{{result=\"{result}\"}} {value}\n"
        ));
    }

    for item in &summary.queue {
        body.push_str(&format!(
            "cex_gateway_saga_queue_commands{{execution_mode=\"{}\",command_kind=\"{}\",status=\"{}\"}} {}\n",
            escape_label(&item.execution_mode),
            escape_label(&item.command_kind),
            escape_label(&item.status),
            item.command_count,
        ));
        body.push_str(&format!(
            "cex_gateway_saga_queue_retry_budget_exhausted{{execution_mode=\"{}\",command_kind=\"{}\",status=\"{}\"}} {}\n",
            escape_label(&item.execution_mode),
            escape_label(&item.command_kind),
            escape_label(&item.status),
            item.retry_budget_exhausted,
        ));
    }

    for (kind, value) in [
        ("orphaned_workflows", summary.reconciliation.orphaned_workflows),
        (
            "observed_status",
            summary.reconciliation.observed_status_mismatches,
        ),
        (
            "observed_execution_id",
            summary.reconciliation.observed_execution_id_mismatches,
        ),
        (
            "reserve_observed_without_receipt",
            summary.reconciliation.reserve_observed_without_receipt,
        ),
        (
            "missing_execution_shadow",
            summary.reconciliation.invocations_missing_execution_shadow,
        ),
    ] {
        body.push_str(&format!(
            "cex_gateway_saga_reconciliation_mismatches{{kind=\"{kind}\"}} {value}\n"
        ));
    }
    body.push_str(&format!(
        "cex_gateway_saga_oldest_shadow_age_seconds {}\n",
        summary.reconciliation.oldest_shadow_age_seconds
    ));
    body
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_are_payload_free_and_include_reconciliation_dimensions() {
        let summary = SagaOperationalSummary {
            service: "gateway-service",
            shadow_write_enabled: true,
            database_configured: true,
            runtime: GatewayRuntimeMetricsSnapshot {
                invocation_create_requests: 0,
                invocation_create_auth_failures: 0,
                invocation_create_capability_failures: 0,
                invocation_create_upstream_failures: 0,
                invocation_get_requests: 0,
                invocation_get_auth_failures: 0,
                execution_approve_requests: 0,
                execution_retry_requests: 0,
                execution_cancel_requests: 0,
                saga_shadow_write_attempts: 3,
                saga_shadow_write_successes: 2,
                saga_shadow_write_failures: 1,
            },
            queue: vec![SagaQueueItem {
                execution_mode: "shadow".to_string(),
                command_kind: "execution_create".to_string(),
                status: "pending".to_string(),
                command_count: 2,
                oldest_available_at: None,
                oldest_lease_expiry: None,
                retry_budget_exhausted: 0,
            }],
            reconciliation: SagaReconciliationSummary {
                shadow_commands_total: 2,
                orphaned_workflows: 1,
                observed_status_mismatches: 0,
                observed_execution_id_mismatches: 0,
                reserve_observed_without_receipt: 0,
                invocations_missing_execution_shadow: 0,
                oldest_shadow_age_seconds: 30,
            },
        };

        let metrics = render_metrics(&summary);
        assert!(metrics.contains("result=\"failure\"} 1"));
        assert!(metrics.contains("kind=\"orphaned_workflows\"} 1"));
        assert!(!metrics.contains("payload"));
        assert!(!metrics.contains("prompt"));
    }
}
