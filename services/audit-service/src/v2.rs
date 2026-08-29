use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminPrincipal,
};
use shared_types::audit_v2::{
    AuditEventCreateRequestV2, AuditEventRecordV2, AUDIT_WRITER_AUTH_SCHEME_V1,
};
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::{service_auth::AuthenticatedService, state::AppState};

#[derive(Debug, Serialize)]
pub struct AuditEventAppendResponseV2 {
    pub replayed: bool,
    pub record: AuditEventRecordV2,
}

pub async fn create_event_v2(
    Extension(writer): Extension<AuthenticatedService>,
    State(state): State<AppState>,
    Json(request): Json<AuditEventCreateRequestV2>,
) -> impl IntoResponse {
    if !writer.authenticated {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "audit v2 requires authenticated workload identity",
                "code": "authenticated_writer_required",
            })),
        )
            .into_response();
    }

    if let Err(error) = request.validate(chrono::Utc::now()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid audit v2 event",
                "message": error.to_string(),
            })),
        )
            .into_response();
    }

    let Some(pool) = &state.pool else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "audit v2 persistence unavailable" })),
        )
            .into_response();
    };

    let append_result = sqlx::query_scalar::<_, Value>(
        "select public.cex_append_audit_event_v2(
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb
        )",
    )
    .bind(request.event_id)
    .bind(request.trace_id)
    .bind(request.org_id)
    .bind(&writer.service_id)
    .bind(AUDIT_WRITER_AUTH_SCHEME_V1)
    .bind(&request.actor_type)
    .bind(&request.actor_id)
    .bind(&request.event_type)
    .bind(&request.schema_version)
    .bind(request.occurred_at)
    .bind(request.payload.to_string())
    .fetch_one(pool)
    .await;

    let value = match append_result {
        Ok(value) => value,
        Err(error) => {
            let collision = error.as_database_error().is_some_and(|database_error| {
                database_error
                    .message()
                    .contains("audit event id collision")
            });
            eprintln!(
                "audit-service: append audit v2 event_id={} failed: {error}",
                request.event_id
            );
            return (
                if collision {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::SERVICE_UNAVAILABLE
                },
                Json(json!({
                    "error": if collision {
                        "audit event id collision"
                    } else {
                        "audit v2 persistence unavailable"
                    }
                })),
            )
                .into_response();
        }
    };

    let replayed = value
        .get("replayed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let record = match value
        .get("record")
        .cloned()
        .ok_or_else(|| "append result missing record".to_string())
        .and_then(|record| {
            serde_json::from_value::<AuditEventRecordV2>(record)
                .map_err(|error| format!("decode appended audit v2 record: {error}"))
        }) {
        Ok(record) => record,
        Err(error) => {
            eprintln!("audit-service: {error}");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "audit v2 response decode failed" })),
            )
                .into_response();
        }
    };

    (
        if replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(AuditEventAppendResponseV2 { replayed, record }),
    )
        .into_response()
}

pub async fn list_events_by_trace_v2(
    Path(trace_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let admin = match authorize_audit_read(&state, &headers) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let Some(pool) = &state.pool else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "audit v2 persistence unavailable" })),
        )
            .into_response();
    };

    let rows = match sqlx::query(
        "select
            event_id,
            trace_id,
            org_id,
            chain_key,
            tenant_sequence,
            previous_event_hash,
            event_hash,
            writer_service_id,
            writer_auth_scheme,
            actor_type,
            actor_id,
            event_type,
            schema_version,
            occurred_at,
            received_at,
            payload
         from public.cex_audit_events_v2
         where trace_id = $1
         order by received_at, event_id",
    )
    .bind(trace_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("audit-service: list audit v2 trace failed: {error}");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "audit v2 query unavailable" })),
            )
                .into_response();
        }
    };

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        match decode_record(row) {
            Ok(record) => events.push(record),
            Err(error) => {
                eprintln!("audit-service: decode audit v2 row failed: {error}");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "audit v2 response decode failed" })),
                )
                    .into_response();
            }
        }
    }

    if let Some(response) = enforce_org_boundary(&admin, &events) {
        return response;
    }
    (StatusCode::OK, Json(events)).into_response()
}

pub async fn metrics_v2(State(state): State<AppState>) -> impl IntoResponse {
    let Some(pool) = &state.pool else {
        return metrics_response("cex_audit_v2_runtime_up 0\n".to_string());
    };

    let counts = sqlx::query(
        "select
            (select count(*)::bigint from public.cex_audit_events_v2) as event_count,
            (select count(*)::bigint from public.cex_audit_chain_heads_v2) as chain_count,
            (select count(*)::bigint from public.cex_audit_outbox_v1
              where status in ('pending', 'claimed', 'retry_wait')) as outbox_backlog,
            (select coalesce(
                extract(epoch from (now() - min(available_at)))::bigint,
                0
             ) from public.cex_audit_outbox_v1
              where status in ('pending', 'claimed', 'retry_wait')) as oldest_outbox_age_seconds",
    )
    .fetch_one(pool)
    .await;

    let row = match counts {
        Ok(row) => row,
        Err(error) => {
            eprintln!("audit-service: audit v2 metrics query failed: {error}");
            return metrics_response("cex_audit_v2_runtime_up 0\n".to_string());
        }
    };

    let body = format!(
        concat!(
            "# HELP cex_audit_v2_runtime_up Whether audit v2 persistence is queryable.\n",
            "# TYPE cex_audit_v2_runtime_up gauge\n",
            "cex_audit_v2_runtime_up 1\n",
            "# HELP cex_audit_v2_events_total Persisted authenticated audit v2 events.\n",
            "# TYPE cex_audit_v2_events_total gauge\n",
            "cex_audit_v2_events_total {event_count}\n",
            "# HELP cex_audit_v2_chains_total Active audit hash-chain heads.\n",
            "# TYPE cex_audit_v2_chains_total gauge\n",
            "cex_audit_v2_chains_total {chain_count}\n",
            "# HELP cex_audit_outbox_backlog Audit outbox records awaiting terminal delivery.\n",
            "# TYPE cex_audit_outbox_backlog gauge\n",
            "cex_audit_outbox_backlog {outbox_backlog}\n",
            "# HELP cex_audit_outbox_oldest_age_seconds Age of the oldest nonterminal audit outbox record.\n",
            "# TYPE cex_audit_outbox_oldest_age_seconds gauge\n",
            "cex_audit_outbox_oldest_age_seconds {oldest_outbox_age_seconds}\n",
        ),
        event_count = row.try_get::<i64, _>("event_count").unwrap_or_default(),
        chain_count = row.try_get::<i64, _>("chain_count").unwrap_or_default(),
        outbox_backlog = row.try_get::<i64, _>("outbox_backlog").unwrap_or_default(),
        oldest_outbox_age_seconds = row
            .try_get::<i64, _>("oldest_outbox_age_seconds")
            .unwrap_or_default(),
    );
    metrics_response(body)
}

fn metrics_response(body: String) -> axum::response::Response {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

fn decode_record(row: PgRow) -> Result<AuditEventRecordV2, String> {
    Ok(AuditEventRecordV2 {
        event_id: row.try_get("event_id").map_err(|error| error.to_string())?,
        trace_id: row.try_get("trace_id").map_err(|error| error.to_string())?,
        org_id: row.try_get("org_id").map_err(|error| error.to_string())?,
        chain_key: row
            .try_get("chain_key")
            .map_err(|error| error.to_string())?,
        tenant_sequence: row
            .try_get("tenant_sequence")
            .map_err(|error| error.to_string())?,
        previous_event_hash: row
            .try_get("previous_event_hash")
            .map_err(|error| error.to_string())?,
        event_hash: row
            .try_get("event_hash")
            .map_err(|error| error.to_string())?,
        writer_service_id: row
            .try_get("writer_service_id")
            .map_err(|error| error.to_string())?,
        writer_auth_scheme: row
            .try_get("writer_auth_scheme")
            .map_err(|error| error.to_string())?,
        actor_type: row
            .try_get("actor_type")
            .map_err(|error| error.to_string())?,
        actor_id: row.try_get("actor_id").map_err(|error| error.to_string())?,
        event_type: row
            .try_get("event_type")
            .map_err(|error| error.to_string())?,
        schema_version: row
            .try_get("schema_version")
            .map_err(|error| error.to_string())?,
        occurred_at: row
            .try_get("occurred_at")
            .map_err(|error| error.to_string())?,
        received_at: row
            .try_get("received_at")
            .map_err(|error| error.to_string())?,
        payload: row.try_get("payload").map_err(|error| error.to_string())?,
    })
}

fn authorize_audit_read(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, axum::response::Response> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        &["audit:read"],
        admin_principal_has_scope,
        "audit admin token not configured",
        Some("configure an audit:read principal to query audit v2 events"),
    )
    .cloned()
    .map_err(|error| error.into_response().into_response())
}

fn enforce_org_boundary(
    admin: &AdminPrincipal,
    events: &[AuditEventRecordV2],
) -> Option<axum::response::Response> {
    if admin.org_ids.is_empty() || events.is_empty() {
        return None;
    }

    let first_org = events.first().and_then(|event| event.org_id);
    if first_org.is_none() || events.iter().any(|event| event.org_id != first_org) {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "audit v2 trace lacks a single org boundary" })),
            )
                .into_response(),
        );
    }
    let org_id = first_org.expect("checked Some").to_string();
    if admin_principal_allows_org(admin, &org_id) {
        None
    } else {
        Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "admin token not authorized for org" })),
            )
                .into_response(),
        )
    }
}
