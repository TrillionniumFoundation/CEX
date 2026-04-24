use shared_config::{build_admin_principal_map, load_audit_scoped_admin_tokens, AdminPrincipal};
use shared_types::{AuditEventCreateRequest, AuditEventRecord};
use sqlx::{PgPool, Row};
use std::{collections::HashMap, env, sync::Arc, vec::Vec};
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub events: Arc<RwLock<Vec<AuditEventRecord>>>,
    pub pool: Option<PgPool>,
    pub fail_fast: bool,
    pub(crate) admin_tokens: Arc<HashMap<String, AdminPrincipal>>,
}

impl AppState {
    pub async fn from_env() -> Self {
        let fail_fast = env_flag("AUDIT_FAIL_FAST", true);
        let pool = match env::var("DATABASE_URL") {
            Ok(database_url) => match PgPool::connect(&database_url).await {
                Ok(pool) => Some(pool),
                Err(err) => {
                    if fail_fast {
                        panic!("audit-service failed to connect postgres: {err}");
                    }
                    eprintln!("audit-service: postgres unavailable, falling back to in-memory events: {err}");
                    None
                }
            },
            Err(_) => {
                if fail_fast {
                    panic!("audit-service requires DATABASE_URL when AUDIT_FAIL_FAST=true");
                }
                eprintln!(
                    "audit-service: DATABASE_URL is not set, falling back to in-memory events"
                );
                None
            }
        };

        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            pool,
            fail_fast,
            admin_tokens: Arc::new(load_admin_tokens()),
        }
    }

    pub fn new_for_tests(fail_fast: bool, admin_token: Option<String>) -> Self {
        let scopes = if admin_token.is_some() {
            vec!["audit:read".to_string()]
        } else {
            Vec::new()
        };
        Self::new_for_tests_with_admin_scopes(fail_fast, admin_token, scopes)
    }

    pub fn new_for_tests_with_admin_scopes(
        fail_fast: bool,
        admin_token: Option<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self::new_for_tests_with_admin_scope_orgs(fail_fast, admin_token, scopes, Vec::new())
    }

    pub fn new_for_tests_with_admin_scope_orgs(
        fail_fast: bool,
        admin_token: Option<String>,
        scopes: Vec<String>,
        org_ids: Vec<String>,
    ) -> Self {
        let admin_tokens = admin_token
            .into_iter()
            .map(|token| {
                (
                    token,
                    AdminPrincipal {
                        actor_id: "test-audit-admin".to_string(),
                        actor_label: Some("Test Audit Admin".to_string()),
                        scopes: scopes.clone(),
                        org_ids: org_ids.clone(),
                    },
                )
            })
            .collect();

        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            pool: None,
            fail_fast,
            admin_tokens: Arc::new(admin_tokens),
        }
    }

    pub async fn create_event(
        &self,
        req: AuditEventCreateRequest,
    ) -> Result<AuditEventRecord, String> {
        let record = AuditEventRecord {
            event_id: Uuid::new_v4(),
            trace_id: req.trace_id,
            org_id: req.org_id,
            actor_type: req.actor_type,
            actor_id: req.actor_id,
            event_type: req.event_type,
            payload: req.payload,
            created_at: chrono::Utc::now(),
        };

        if let Some(pool) = &self.pool {
            insert_event(pool, &record).await?;
            return Ok(record);
        }

        if self.fail_fast {
            return Err("audit postgres pool not initialized".to_string());
        }

        self.events.write().await.push(record.clone());
        Ok(record)
    }

    pub async fn list_events_by_trace(
        &self,
        trace_id: Uuid,
    ) -> Result<Vec<AuditEventRecord>, String> {
        if let Some(pool) = &self.pool {
            return load_events_by_trace(pool, trace_id).await;
        }

        if self.fail_fast {
            return Err("audit postgres pool not initialized".to_string());
        }

        let events = self
            .events
            .read()
            .await
            .iter()
            .filter(|event| event.trace_id == trace_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(events)
    }
}

async fn insert_event(pool: &PgPool, record: &AuditEventRecord) -> Result<(), String> {
    let payload_text = serde_json::to_string(&record.payload)
        .map_err(|e| format!("serialize audit payload failed: {e}"))?;

    sqlx::query(
        "insert into audit_events (event_id, trace_id, org_id, actor_type, actor_id, event_type, payload, created_at) values ($1, $2, $3::uuid, $4, $5, $6, $7::jsonb, $8)"
    )
    .bind(record.event_id)
    .bind(record.trace_id)
    .bind(&record.org_id)
    .bind(&record.actor_type)
    .bind(&record.actor_id)
    .bind(&record.event_type)
    .bind(payload_text)
    .bind(record.created_at)
    .execute(pool)
    .await
    .map_err(|e| format!("insert audit event failed: {e}"))?;

    Ok(())
}

async fn load_events_by_trace(
    pool: &PgPool,
    trace_id: Uuid,
) -> Result<Vec<AuditEventRecord>, String> {
    let rows = sqlx::query(
        "select event_id, trace_id, org_id::text as org_id, actor_type, actor_id, event_type, coalesce(payload::text, 'null') as payload_text, created_at from audit_events where trace_id = $1 order by created_at asc, event_id asc"
    )
    .bind(trace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("query audit events failed: {e}"))?;

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let payload_text: String = row
            .try_get("payload_text")
            .map_err(|e| format!("read audit payload text failed: {e}"))?;
        let payload = serde_json::from_str(&payload_text)
            .map_err(|e| format!("decode audit payload failed: {e}; payload={payload_text}"))?;

        events.push(AuditEventRecord {
            event_id: row
                .try_get("event_id")
                .map_err(|e| format!("read event_id failed: {e}"))?,
            trace_id: row
                .try_get("trace_id")
                .map_err(|e| format!("read trace_id failed: {e}"))?,
            org_id: row
                .try_get("org_id")
                .map_err(|e| format!("read org_id failed: {e}"))?,
            actor_type: row
                .try_get("actor_type")
                .map_err(|e| format!("read actor_type failed: {e}"))?,
            actor_id: row
                .try_get("actor_id")
                .map_err(|e| format!("read actor_id failed: {e}"))?,
            event_type: row
                .try_get("event_type")
                .map_err(|e| format!("read event_type failed: {e}"))?,
            payload,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("read created_at failed: {e}"))?,
        });
    }

    Ok(events)
}

fn env_flag(name: &str, default_value: bool) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default_value,
    }
}

fn load_admin_tokens() -> HashMap<String, AdminPrincipal> {
    build_admin_principal_map(load_audit_scoped_admin_tokens())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn build_admin_token_map_preserves_scopes() {
        let map = build_admin_principal_map(vec![shared_config::ScopedAdminToken {
            token: "audit-token".to_string(),
            actor_id: "audit-reader".to_string(),
            actor_label: Some("Audit Reader".to_string()),
            scopes: vec!["audit:read".to_string(), "audit:export".to_string()],
            org_ids: Vec::new(),
        }]);

        let admin = map.get("audit-token").expect("audit token");
        assert_eq!(admin.actor_id, "audit-reader");
        assert_eq!(admin.actor_label.as_deref(), Some("Audit Reader"));
        assert!(shared_config::admin_principal_has_scope(
            admin,
            "audit:read"
        ));
        assert!(shared_config::admin_principal_has_scope(
            admin,
            "audit:export"
        ));
        assert!(!shared_config::admin_principal_has_scope(
            admin,
            "api_keys:manage"
        ));
    }

    #[test]
    fn load_admin_tokens_defaults_in_dev() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::remove_var("AUDIT_ADMIN_TOKEN");
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::set_var("APP_ENV", "dev");
        }

        let tokens = load_admin_tokens();
        let admin = tokens
            .get("local-dev-admin-token")
            .expect("default dev audit token");
        assert!(shared_config::admin_principal_has_scope(
            admin,
            "audit:read"
        ));
        assert!(shared_config::admin_principal_has_scope(
            admin,
            "api_keys:manage"
        ));

        unsafe {
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn load_admin_tokens_prefers_audit_specific_json_over_shared_identity_json() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::set_var(
                "IDENTITY_ADMIN_TOKENS_JSON",
                r#"[{"token":"shared-token","actor_id":"shared-admin","actor_label":"Shared Admin","scopes":["api_keys:manage","audit:read"]}]"#,
            );
            env::set_var(
                "AUDIT_ADMIN_TOKENS_JSON",
                r#"[{"token":"audit-only-token","actor_id":"audit-reader","actor_label":"Audit Reader","scopes":["audit:read"]}]"#,
            );
            env::remove_var("AUDIT_ADMIN_TOKEN");
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::set_var("APP_ENV", "prod");
        }

        let tokens = load_admin_tokens();
        assert!(tokens.contains_key("audit-only-token"));
        assert!(!tokens.contains_key("shared-token"));
        let admin = tokens
            .get("audit-only-token")
            .expect("audit specific token");
        assert_eq!(admin.actor_id, "audit-reader");
        assert!(shared_config::admin_principal_has_scope(
            admin,
            "audit:read"
        ));
        assert!(!shared_config::admin_principal_has_scope(
            admin,
            "api_keys:manage"
        ));

        unsafe {
            env::remove_var("AUDIT_ADMIN_TOKENS_JSON");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::remove_var("APP_ENV");
        }
    }
}
