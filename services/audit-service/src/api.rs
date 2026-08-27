use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::{json, Value};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    AdminPrincipal,
};
use shared_types::{AuditEventCreateRequest, AuditEventRecord};
use uuid::Uuid;

use crate::{service_auth::AuthenticatedService, state::AppState};

const AUDIT_WRITER_METADATA_KEY: &str = "_cex_audit_writer";

pub async fn health() -> &'static str {
    "audit-service ok"
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let event_buffer_count = state.events.read().await.len();
    let body = format!(
        concat!(
            "# HELP cex_audit_service_up Whether audit-service metrics are being served.\n",
            "# TYPE cex_audit_service_up gauge\n",
            "cex_audit_service_up 1\n",
            "# HELP cex_audit_event_buffer_records_total In-memory audit event buffer records.\n",
            "# TYPE cex_audit_event_buffer_records_total gauge\n",
            "cex_audit_event_buffer_records_total {event_buffer_count}\n",
            "# HELP cex_audit_postgres_configured Whether audit-service has a postgres pool.\n",
            "# TYPE cex_audit_postgres_configured gauge\n",
            "cex_audit_postgres_configured {postgres_configured}\n",
            "# HELP cex_audit_admin_tokens_total Audit admin tokens currently loaded.\n",
            "# TYPE cex_audit_admin_tokens_total gauge\n",
            "cex_audit_admin_tokens_total {admin_tokens}\n",
        ),
        event_buffer_count = event_buffer_count,
        postgres_configured = if state.pool.is_some() { 1 } else { 0 },
        admin_tokens = state.admin_tokens.len(),
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

pub async fn create_event(
    Extension(writer): Extension<AuthenticatedService>,
    State(state): State<AppState>,
    Json(mut req): Json<AuditEventCreateRequest>,
) -> impl IntoResponse {
    bind_authenticated_writer(&mut req, &writer);

    match state.create_event(req).await {
        Ok(record) => (StatusCode::CREATED, Json(record)).into_response(),
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

fn bind_authenticated_writer(
    request: &mut AuditEventCreateRequest,
    writer: &AuthenticatedService,
) {
    if !writer.authenticated {
        return;
    }

    let metadata = json!({
        "service_id": writer.service_id,
        "authentication": "workload-token-v1",
    });

    match &mut request.payload {
        Value::Object(payload) => {
            // The server owns this reserved key. Any caller-supplied value is replaced.
            payload.insert(AUDIT_WRITER_METADATA_KEY.to_string(), metadata);
        }
        payload => {
            let original = std::mem::replace(payload, Value::Null);
            *payload = json!({
                "event_payload": original,
                AUDIT_WRITER_METADATA_KEY: metadata,
            });
        }
    }
}

pub async fn list_events_by_trace(
    Path(trace_id): Path<Uuid>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let admin = match authorize_audit_read(&state, &headers) {
        Ok(admin) => admin,
        Err(response) => return response.into_response(),
    };

    match state.list_events_by_trace(trace_id).await {
        Ok(events) => {
            if let Some(response) = enforce_trace_org_boundary(&admin, &events) {
                return response.into_response();
            }
            (StatusCode::OK, Json(events)).into_response()
        }
        Err(err) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": err })),
        )
            .into_response(),
    }
}

fn authorize_audit_read(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AdminPrincipal, (StatusCode, Json<serde_json::Value>)> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        &["audit:read"],
        admin_principal_has_scope,
        "audit admin token not configured",
        Some("set AUDIT_ADMIN_TOKENS_JSON, AUDIT_ADMIN_TOKEN, or the shared identity admin token env to enable audit reads"),
    )
    .cloned()
    .map_err(|err| err.into_response())
}

fn enforce_trace_org_boundary(
    admin: &AdminPrincipal,
    events: &[AuditEventRecord],
) -> Option<(StatusCode, Json<serde_json::Value>)> {
    if admin.org_ids.is_empty() || events.is_empty() {
        return None;
    }

    let mut trace_org_id: Option<&str> = None;
    for event in events {
        let event_org_id = match event.org_id.as_deref() {
            Some(org_id) => org_id,
            None => {
                return Some((
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "error": "audit trace missing org boundary metadata"
                    })),
                ))
            }
        };

        match trace_org_id {
            None => trace_org_id = Some(event_org_id),
            Some(existing) if existing == event_org_id => {}
            Some(_) => {
                return Some((
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "error": "audit trace spans multiple orgs"
                    })),
                ))
            }
        }
    }

    let trace_org_id = trace_org_id.expect("non-empty events must yield trace org id");
    if admin_principal_allows_org(admin, trace_org_id) {
        None
    } else {
        Some((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "admin token not authorized for org",
                "message": trace_org_id,
            })),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(actor_type: &str, payload: Value) -> AuditEventCreateRequest {
        AuditEventCreateRequest {
            trace_id: Uuid::new_v4(),
            org_id: Some(Uuid::new_v4().to_string()),
            actor_type: actor_type.to_string(),
            actor_id: Some("operator".to_string()),
            event_type: "test.event".to_string(),
            payload,
        }
    }

    #[test]
    fn authenticated_writer_preserves_domain_actor_and_overrides_reserved_metadata() {
        let mut req = request(
            "policy-engine",
            json!({
                "ok": true,
                AUDIT_WRITER_METADATA_KEY: {"service_id": "spoofed-service"}
            }),
        );
        bind_authenticated_writer(
            &mut req,
            &AuthenticatedService {
                service_id: "execution-service".to_string(),
                authenticated: true,
            },
        );

        assert_eq!(req.actor_type, "policy-engine");
        assert_eq!(
            req.payload[AUDIT_WRITER_METADATA_KEY]["service_id"],
            "execution-service"
        );
        assert_eq!(
            req.payload[AUDIT_WRITER_METADATA_KEY]["authentication"],
            "workload-token-v1"
        );
    }

    #[test]
    fn scalar_payload_is_wrapped_without_losing_original_value() {
        let mut req = request("gateway-service", json!("original"));
        bind_authenticated_writer(
            &mut req,
            &AuthenticatedService {
                service_id: "gateway-service".to_string(),
                authenticated: true,
            },
        );
        assert_eq!(req.payload["event_payload"], "original");
        assert_eq!(
            req.payload[AUDIT_WRITER_METADATA_KEY]["service_id"],
            "gateway-service"
        );
    }

    #[test]
    fn compatibility_mode_preserves_legacy_payload() {
        let original = json!({"ok": true});
        let mut req = request("legacy-test-writer", original.clone());
        bind_authenticated_writer(
            &mut req,
            &AuthenticatedService {
                service_id: "compatibility-unauthenticated".to_string(),
                authenticated: false,
            },
        );
        assert_eq!(req.actor_type, "legacy-test-writer");
        assert_eq!(req.payload, original);
    }
}
