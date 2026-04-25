use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use shared_config::{
    admin_principal_allows_org, admin_principal_has_scope, authorize_scoped_admin_from_map,
    build_admin_principal_map, load_identity_scoped_admin_tokens, AdminPrincipal,
};
use shared_types::{ApiKeyResolveRequest, AuditEventCreateRequest, AuthContext};
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    PgPool, Row,
};
use std::{collections::HashMap, env, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    api_keys: Arc<HashMap<String, AuthContext>>,
    pool: Option<PgPool>,
    admin_tokens: Arc<HashMap<String, AdminPrincipal>>,
    audit_base_url: Option<String>,
    http: reqwest::Client,
}

impl AppState {
    pub async fn from_env() -> Self {
        Self {
            api_keys: Arc::new(load_api_keys()),
            pool: maybe_connect_pool().await,
            admin_tokens: Arc::new(load_identity_admin_tokens()),
            audit_base_url: load_audit_base_url(),
            http: reqwest::Client::new(),
        }
    }

    pub fn new_for_tests(
        api_keys: HashMap<String, AuthContext>,
        admin_token: Option<String>,
        pool: Option<PgPool>,
    ) -> Self {
        Self::new_for_tests_with_audit(api_keys, admin_token, pool, None)
    }

    pub fn new_for_tests_with_admin_scopes(
        api_keys: HashMap<String, AuthContext>,
        admin_token: Option<String>,
        pool: Option<PgPool>,
        audit_base_url: Option<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self::new_for_tests_with_admin_scope_orgs(
            api_keys,
            admin_token,
            pool,
            audit_base_url,
            scopes,
            Vec::new(),
        )
    }

    pub fn new_for_tests_with_admin_scope_orgs(
        api_keys: HashMap<String, AuthContext>,
        admin_token: Option<String>,
        pool: Option<PgPool>,
        audit_base_url: Option<String>,
        scopes: Vec<String>,
        org_ids: Vec<String>,
    ) -> Self {
        let admin_tokens = admin_token
            .into_iter()
            .map(|token| {
                (
                    token,
                    AdminPrincipal {
                        actor_id: "test-admin".to_string(),
                        actor_label: Some("Test Admin".to_string()),
                        scopes: scopes.clone(),
                        org_ids: org_ids.clone(),
                    },
                )
            })
            .collect();

        Self::new_for_tests_with_admins(api_keys, admin_tokens, pool, audit_base_url)
    }

    pub fn new_for_tests_with_audit(
        api_keys: HashMap<String, AuthContext>,
        admin_token: Option<String>,
        pool: Option<PgPool>,
        audit_base_url: Option<String>,
    ) -> Self {
        Self::new_for_tests_with_admin_scopes(
            api_keys,
            admin_token,
            pool,
            audit_base_url,
            vec!["api_keys:manage".to_string(), "audit:read".to_string()],
        )
    }

    fn new_for_tests_with_admins(
        api_keys: HashMap<String, AuthContext>,
        admin_tokens: HashMap<String, AdminPrincipal>,
        pool: Option<PgPool>,
        audit_base_url: Option<String>,
    ) -> Self {
        Self {
            api_keys: Arc::new(api_keys),
            pool,
            admin_tokens: Arc::new(admin_tokens),
            audit_base_url,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct StaticApiKeyRecord {
    api_key: String,
    org_id: String,
    actor_id: Option<String>,
    actor_label: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct IssueApiKeyRequest {
    org_id: String,
    user_id: Option<String>,
    label: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct ListApiKeysQuery {
    org_id: String,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct RevokeApiKeyRequest {
    reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ApiKeyRecordView {
    api_key_id: Uuid,
    org_id: String,
    user_id: Option<String>,
    key_prefix: String,
    label: Option<String>,
    status: String,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_reason: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct IssueApiKeyResponse {
    api_key: String,
    record: ApiKeyRecordView,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ApiKeyListResponse {
    items: Vec<ApiKeyRecordView>,
}

#[derive(Debug, Clone)]
enum ResolveApiKeyLookup {
    Authorized(AuthContext),
    NotFound,
    Rejected(String),
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "identity-service ok" }))
        .route("/metrics", get(metrics))
        .route("/v1/auth/resolve", post(resolve_api_key))
        .route("/v1/api-keys", get(list_api_keys).post(issue_api_key))
        .route("/v1/api-keys/:id/revoke", post(revoke_api_key))
        .with_state(state)
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = format!(
        concat!(
            "# HELP cex_identity_service_up Whether identity-service metrics are being served.\n",
            "# TYPE cex_identity_service_up gauge\n",
            "cex_identity_service_up 1\n",
            "# HELP cex_identity_static_api_keys_total Static API keys currently loaded.\n",
            "# TYPE cex_identity_static_api_keys_total gauge\n",
            "cex_identity_static_api_keys_total {static_api_keys}\n",
            "# HELP cex_identity_admin_tokens_total Identity admin tokens currently loaded.\n",
            "# TYPE cex_identity_admin_tokens_total gauge\n",
            "cex_identity_admin_tokens_total {admin_tokens}\n",
            "# HELP cex_identity_postgres_configured Whether identity-service has a postgres pool.\n",
            "# TYPE cex_identity_postgres_configured gauge\n",
            "cex_identity_postgres_configured {postgres_configured}\n",
            "# HELP cex_identity_audit_client_configured Whether identity-service audit client is configured.\n",
            "# TYPE cex_identity_audit_client_configured gauge\n",
            "cex_identity_audit_client_configured {audit_configured}\n",
        ),
        static_api_keys = state.api_keys.len(),
        admin_tokens = state.admin_tokens.len(),
        postgres_configured = if state.pool.is_some() { 1 } else { 0 },
        audit_configured = if state.audit_base_url.is_some() { 1 } else { 0 },
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn resolve_api_key(
    State(state): State<AppState>,
    Json(req): Json<ApiKeyResolveRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let static_ctx = state.api_keys.get(&req.api_key).cloned();

    if let Some(pool) = &state.pool {
        match resolve_api_key_from_db(pool, &req.api_key).await {
            Ok(ResolveApiKeyLookup::Authorized(ctx)) => return ok_response(ctx),
            Ok(ResolveApiKeyLookup::Rejected(reason)) => {
                return error_response(StatusCode::UNAUTHORIZED, reason, None)
            }
            Ok(ResolveApiKeyLookup::NotFound) => {}
            Err(message) => {
                if let Some(ctx) = static_ctx {
                    return ok_response(ctx);
                }
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "auth backend unavailable",
                    Some(message),
                );
            }
        }
    }

    match static_ctx {
        Some(ctx) => ok_response(ctx),
        None => error_response(StatusCode::UNAUTHORIZED, "invalid api key", None),
    }
}

async fn issue_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<IssueApiKeyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let admin = match authorize_admin(&state, &headers, "api_keys:manage") {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let org_id = match parse_uuid_field("org_id", &req.org_id) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    if !admin_principal_allows_org(&admin, &org_id.to_string()) {
        return error_response(
            StatusCode::FORBIDDEN,
            "admin token not authorized for org",
            Some(org_id.to_string()),
        );
    }

    let Some(pool) = &state.pool else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "database-backed api key management unavailable",
            Some("configure DATABASE_URL to enable issue/list/revoke operations".to_string()),
        );
    };

    let user_id = match req.user_id.as_deref() {
        Some(raw) => match parse_uuid_field("user_id", raw) {
            Ok(value) => Some(value),
            Err(resp) => return resp,
        },
        None => None,
    };

    if let Some(user_id) = user_id {
        if let Err(message) = ensure_user_belongs_to_org(pool, user_id, org_id).await {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid user provenance",
                Some(message),
            );
        }
    }

    let api_key = generate_api_key_value();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = derive_key_prefix(&api_key);

    let row = match sqlx::query(
        r#"
insert into api_keys (org_id, user_id, key_hash, key_prefix, label, status, expires_at)
values ($1, $2, $3, $4, $5, 'active', $6)
returning
    api_key_id,
    org_id::text as org_id,
    user_id::text as user_id,
    key_prefix,
    label,
    status,
    expires_at,
    last_used_at,
    revoked_at,
    revoked_reason,
    created_at
        "#,
    )
    .bind(org_id)
    .bind(user_id)
    .bind(key_hash)
    .bind(&key_prefix)
    .bind(req.label)
    .bind(req.expires_at)
    .fetch_one(pool)
    .await
    {
        Ok(row) => row,
        Err(err) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "api key insert failed",
                Some(err.to_string()),
            )
        }
    };

    let record = match decode_api_key_record(row) {
        Ok(record) => record,
        Err(message) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "api key decode failed",
                Some(message),
            )
        }
    };

    best_effort_audit(
        &state,
        record.api_key_id,
        Some(record.org_id.clone()),
        Some(admin.actor_id.clone()),
        "identity.api_key.issued",
        json!({
            "api_key_id": record.api_key_id,
            "org_id": record.org_id,
            "user_id": record.user_id,
            "label": record.label,
            "key_prefix": record.key_prefix,
            "expires_at": record.expires_at,
            "admin_actor_id": admin.actor_id,
            "admin_actor_label": admin.actor_label,
        }),
    )
    .await;

    (
        StatusCode::CREATED,
        Json(
            serde_json::to_value(IssueApiKeyResponse { api_key, record })
                .expect("serialize issue api key response"),
        ),
    )
}

async fn list_api_keys(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListApiKeysQuery>,
) -> (StatusCode, Json<serde_json::Value>) {
    let admin = match authorize_admin_with_any_scope(
        &state,
        &headers,
        &["api_keys:read", "api_keys:manage"],
    ) {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let org_id = match parse_uuid_field("org_id", &query.org_id) {
        Ok(value) => value,
        Err(resp) => return resp,
    };

    if !admin_principal_allows_org(&admin, &org_id.to_string()) {
        return error_response(
            StatusCode::FORBIDDEN,
            "admin token not authorized for org",
            Some(org_id.to_string()),
        );
    }

    let Some(pool) = &state.pool else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "database-backed api key management unavailable",
            Some("configure DATABASE_URL to enable issue/list/revoke operations".to_string()),
        );
    };

    let rows = match sqlx::query(
        r#"
select
    api_key_id,
    org_id::text as org_id,
    user_id::text as user_id,
    key_prefix,
    label,
    status,
    expires_at,
    last_used_at,
    revoked_at,
    revoked_reason,
    created_at
from api_keys
where org_id = $1
order by created_at desc
        "#,
    )
    .bind(org_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "api key list failed",
                Some(err.to_string()),
            )
        }
    };

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        match decode_api_key_record(row) {
            Ok(record) => items.push(record),
            Err(message) => {
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "api key decode failed",
                    Some(message),
                )
            }
        }
    }

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(ApiKeyListResponse { items })
                .expect("serialize list api key response"),
        ),
    )
}

async fn revoke_api_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(req): Json<RevokeApiKeyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let admin = match authorize_admin(&state, &headers, "api_keys:manage") {
        Ok(admin) => admin,
        Err(response) => return response,
    };
    let Some(pool) = &state.pool else {
        return error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "database-backed api key management unavailable",
            Some("configure DATABASE_URL to enable issue/list/revoke operations".to_string()),
        );
    };

    let target_org_id =
        match sqlx::query_scalar::<_, Uuid>("select org_id from api_keys where api_key_id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
        {
            Ok(Some(org_id)) => org_id,
            Ok(None) => return error_response(StatusCode::NOT_FOUND, "api key not found", None),
            Err(err) => {
                return error_response(
                    StatusCode::BAD_GATEWAY,
                    "api key lookup failed",
                    Some(err.to_string()),
                )
            }
        };

    if !admin_principal_allows_org(&admin, &target_org_id.to_string()) {
        return error_response(
            StatusCode::FORBIDDEN,
            "admin token not authorized for org",
            Some(target_org_id.to_string()),
        );
    }

    let row = match sqlx::query(
        r#"
update api_keys
set status = 'revoked',
    revoked_at = coalesce(revoked_at, now()),
    revoked_reason = coalesce(revoked_reason, $2)
where api_key_id = $1
returning
    api_key_id,
    org_id::text as org_id,
    user_id::text as user_id,
    key_prefix,
    label,
    status,
    expires_at,
    last_used_at,
    revoked_at,
    revoked_reason,
    created_at
        "#,
    )
    .bind(id)
    .bind(req.reason.as_deref())
    .fetch_one(pool)
    .await
    {
        Ok(row) => row,
        Err(err) => {
            return error_response(
                StatusCode::BAD_GATEWAY,
                "api key revoke failed",
                Some(err.to_string()),
            )
        }
    };

    match decode_api_key_record(row) {
        Ok(record) => {
            best_effort_audit(
                &state,
                record.api_key_id,
                Some(record.org_id.clone()),
                Some(admin.actor_id.clone()),
                "identity.api_key.revoked",
                json!({
                    "api_key_id": record.api_key_id,
                    "org_id": record.org_id,
                    "user_id": record.user_id,
                    "label": record.label,
                    "key_prefix": record.key_prefix,
                    "revoked_reason": record.revoked_reason,
                    "admin_actor_id": admin.actor_id,
                    "admin_actor_label": admin.actor_label,
                }),
            )
            .await;

            (
                StatusCode::OK,
                Json(serde_json::to_value(record).expect("serialize revoked api key response")),
            )
        }
        Err(message) => error_response(
            StatusCode::BAD_GATEWAY,
            "api key decode failed",
            Some(message),
        ),
    }
}

fn ok_response(ctx: AuthContext) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::to_value(ctx).expect("serialize auth context")),
    )
}

async fn best_effort_audit(
    state: &AppState,
    trace_id: Uuid,
    org_id: Option<String>,
    actor_id: Option<String>,
    event_type: &str,
    payload: serde_json::Value,
) {
    let Some(audit_base_url) = state.audit_base_url.as_deref() else {
        return;
    };

    let request = AuditEventCreateRequest {
        trace_id,
        org_id,
        actor_type: "identity-service".to_string(),
        actor_id,
        event_type: event_type.to_string(),
        payload,
    };

    let _ = state
        .http
        .post(format!("{audit_base_url}/v1/audit/events"))
        .json(&request)
        .send()
        .await;
}

fn authorize_admin(
    state: &AppState,
    headers: &HeaderMap,
    required_scope: &str,
) -> Result<AdminPrincipal, (StatusCode, Json<serde_json::Value>)> {
    authorize_admin_with_any_scope(state, headers, &[required_scope])
}

fn authorize_admin_with_any_scope(
    state: &AppState,
    headers: &HeaderMap,
    required_scopes: &[&str],
) -> Result<AdminPrincipal, (StatusCode, Json<serde_json::Value>)> {
    authorize_scoped_admin_from_map(
        headers,
        &state.admin_tokens,
        required_scopes,
        |admin, scope| admin_principal_has_scope(admin, scope),
        "identity admin token not configured",
        Some("set IDENTITY_ADMIN_TOKENS_JSON or IDENTITY_ADMIN_TOKEN to enable key management endpoints"),
    )
    .cloned()
    .map_err(|err| err.into_response())
}

fn load_audit_base_url() -> Option<String> {
    env::var("AUDIT_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn load_identity_admin_tokens() -> HashMap<String, AdminPrincipal> {
    build_admin_principal_map(load_identity_scoped_admin_tokens())
}

fn error_response(
    status: StatusCode,
    error: impl Into<String>,
    message: Option<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let error = error.into();
    let payload = match message {
        Some(message) => serde_json::json!({ "error": error, "message": message }),
        None => serde_json::json!({ "error": error }),
    };
    (status, Json(payload))
}

async fn maybe_connect_pool() -> Option<PgPool> {
    let database_url = env::var("DATABASE_URL").ok()?;

    match PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
    {
        Ok(pool) => Some(pool),
        Err(err) => {
            eprintln!(
                "identity-service: DATABASE_URL connect failed, falling back to static API keys: {err}"
            );
            None
        }
    }
}

async fn resolve_api_key_from_db(
    pool: &PgPool,
    api_key: &str,
) -> Result<ResolveApiKeyLookup, String> {
    let key_hash = hash_api_key(api_key);
    let row = sqlx::query(
        r#"
select
    api_key_id,
    org_id::text as org_id,
    user_id::text as actor_id,
    coalesce(nullif(label, ''), concat('API key ', key_prefix)) as actor_label,
    status,
    expires_at,
    revoked_at
from api_keys
where key_hash = $1
limit 1
        "#,
    )
    .bind(key_hash)
    .fetch_optional(pool)
    .await
    .map_err(|err| format!("db api_key lookup failed: {err}"))?;

    let Some(row) = row else {
        return Ok(ResolveApiKeyLookup::NotFound);
    };

    let api_key_id = row
        .try_get::<Uuid, _>("api_key_id")
        .map_err(|err| format!("db api_key id decode failed: {err}"))?;
    let org_id = row
        .try_get::<String, _>("org_id")
        .map_err(|err| format!("db api_key org_id decode failed: {err}"))?;
    let actor_id = row
        .try_get::<Option<String>, _>("actor_id")
        .map_err(|err| format!("db api_key actor_id decode failed: {err}"))?;
    let actor_label = row
        .try_get::<Option<String>, _>("actor_label")
        .map_err(|err| format!("db api_key actor_label decode failed: {err}"))?;
    let status = row
        .try_get::<String, _>("status")
        .map_err(|err| format!("db api_key status decode failed: {err}"))?;
    let expires_at = row
        .try_get::<Option<DateTime<Utc>>, _>("expires_at")
        .map_err(|err| format!("db api_key expires_at decode failed: {err}"))?;
    let revoked_at = row
        .try_get::<Option<DateTime<Utc>>, _>("revoked_at")
        .map_err(|err| format!("db api_key revoked_at decode failed: {err}"))?;

    match normalize_api_key_status(&status, expires_at, revoked_at, Utc::now()).as_str() {
        "active" => {
            sqlx::query("update api_keys set last_used_at = now() where api_key_id = $1")
                .bind(api_key_id)
                .execute(pool)
                .await
                .map_err(|err| format!("db api_key last_used_at update failed: {err}"))?;

            Ok(ResolveApiKeyLookup::Authorized(AuthContext {
                org_id,
                actor_id,
                actor_label,
            }))
        }
        "revoked" => Ok(ResolveApiKeyLookup::Rejected("revoked api key".to_string())),
        "expired" => Ok(ResolveApiKeyLookup::Rejected("expired api key".to_string())),
        other => Ok(ResolveApiKeyLookup::Rejected(format!(
            "inactive api key status: {other}"
        ))),
    }
}

async fn ensure_user_belongs_to_org(
    pool: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<(), String> {
    let count = sqlx::query_scalar::<_, i64>(
        "select count(*) from users where user_id = $1 and org_id = $2",
    )
    .bind(user_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .map_err(|err| format!("user provenance lookup failed: {err}"))?;

    if count == 0 {
        return Err(format!(
            "user {} does not belong to org {}",
            user_id, org_id
        ));
    }

    Ok(())
}

fn decode_api_key_record(row: PgRow) -> Result<ApiKeyRecordView, String> {
    let api_key_id = row
        .try_get("api_key_id")
        .map_err(|err| format!("read api_key_id failed: {err}"))?;
    let org_id = row
        .try_get("org_id")
        .map_err(|err| format!("read org_id failed: {err}"))?;
    let user_id = row
        .try_get("user_id")
        .map_err(|err| format!("read user_id failed: {err}"))?;
    let key_prefix = row
        .try_get("key_prefix")
        .map_err(|err| format!("read key_prefix failed: {err}"))?;
    let label = row
        .try_get("label")
        .map_err(|err| format!("read label failed: {err}"))?;
    let status = row
        .try_get::<String, _>("status")
        .map_err(|err| format!("read status failed: {err}"))?;
    let expires_at = row
        .try_get("expires_at")
        .map_err(|err| format!("read expires_at failed: {err}"))?;
    let last_used_at = row
        .try_get("last_used_at")
        .map_err(|err| format!("read last_used_at failed: {err}"))?;
    let revoked_at = row
        .try_get("revoked_at")
        .map_err(|err| format!("read revoked_at failed: {err}"))?;
    let revoked_reason = row
        .try_get("revoked_reason")
        .map_err(|err| format!("read revoked_reason failed: {err}"))?;
    let created_at = row
        .try_get("created_at")
        .map_err(|err| format!("read created_at failed: {err}"))?;

    Ok(ApiKeyRecordView {
        api_key_id,
        org_id,
        user_id,
        key_prefix,
        label,
        status: normalize_api_key_status(&status, expires_at, revoked_at, Utc::now()),
        expires_at,
        last_used_at,
        revoked_at,
        revoked_reason,
        created_at,
    })
}

fn load_api_keys() -> HashMap<String, AuthContext> {
    let records = env::var("IDENTITY_STATIC_API_KEYS_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<StaticApiKeyRecord>>(&raw).ok())
        .unwrap_or_default();

    build_static_api_key_map(records)
}

fn build_static_api_key_map(records: Vec<StaticApiKeyRecord>) -> HashMap<String, AuthContext> {
    let mut map = HashMap::new();

    for record in records {
        map.insert(
            record.api_key,
            AuthContext {
                org_id: record.org_id,
                actor_id: record.actor_id,
                actor_label: record.actor_label,
            },
        );
    }

    if map.is_empty() {
        map.insert(
            "local-dev-key".to_string(),
            AuthContext {
                org_id: "00000000-0000-0000-0000-00000000ce01".to_string(),
                actor_id: Some("local-dev-actor".to_string()),
                actor_label: Some("Local Dev".to_string()),
            },
        );
    }

    map
}

fn parse_uuid_field(field: &str, raw: &str) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    Uuid::parse_str(raw).map_err(|err| {
        error_response(
            StatusCode::BAD_REQUEST,
            format!("invalid {field}"),
            Some(err.to_string()),
        )
    })
}

fn normalize_api_key_status(
    status: &str,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    if revoked_at.is_some() || status.eq_ignore_ascii_case("revoked") {
        return "revoked".to_string();
    }

    if expires_at.is_some_and(|expires_at| expires_at <= now) {
        return "expired".to_string();
    }

    status.to_ascii_lowercase()
}

fn generate_api_key_value() -> String {
    format!(
        "cex_pk_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn derive_key_prefix(api_key: &str) -> String {
    api_key.chars().take(16).collect()
}

fn hash_api_key(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State,
        http::{header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode as AxumStatusCode},
        response::IntoResponse,
        routing::post,
        Json, Router,
    };
    use std::sync::{Arc, Mutex as StdMutex, OnceLock};
    use tokio::{net::TcpListener, sync::Mutex};

    fn env_lock() -> &'static StdMutex<()> {
        static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| StdMutex::new(()))
    }

    #[test]
    fn hash_api_key_is_stable_sha256_hex() {
        assert_eq!(
            hash_api_key("local-dev-key"),
            "ed5a18fb8f807f996d649e379d3f35f39c543a91bdbf88c492f2ebd10d4df86c"
        );
    }

    #[test]
    fn derive_key_prefix_redacts_long_key() {
        assert_eq!(
            derive_key_prefix("cex_pk_1234567890abcdef_deadbeef"),
            "cex_pk_123456789"
        );
    }

    #[test]
    fn generate_api_key_value_uses_expected_prefix() {
        let key = generate_api_key_value();
        assert!(key.starts_with("cex_pk_"));
        assert!(key.len() > 40);
    }

    #[test]
    fn normalize_api_key_status_marks_expired_keys() {
        let now = Utc::now();
        assert_eq!(
            normalize_api_key_status(
                "active",
                Some(now - chrono::TimeDelta::seconds(1)),
                None,
                now
            ),
            "expired"
        );
    }

    #[test]
    fn normalize_api_key_status_prefers_revoked_over_expired() {
        let now = Utc::now();
        assert_eq!(
            normalize_api_key_status(
                "active",
                Some(now - chrono::TimeDelta::seconds(1)),
                Some(now - chrono::TimeDelta::seconds(5)),
                now,
            ),
            "revoked"
        );
    }

    #[test]
    fn build_static_api_key_map_falls_back_to_local_dev_key() {
        let map = build_static_api_key_map(Vec::new());
        let ctx = map.get("local-dev-key").expect("default local-dev key");
        assert_eq!(ctx.org_id, "00000000-0000-0000-0000-00000000ce01");
        assert_eq!(ctx.actor_id.as_deref(), Some("local-dev-actor"));
    }

    #[test]
    fn build_static_api_key_map_keeps_explicit_records() {
        let map = build_static_api_key_map(vec![StaticApiKeyRecord {
            api_key: "tenant-key".to_string(),
            org_id: "org-123".to_string(),
            actor_id: Some("actor-456".to_string()),
            actor_label: Some("Tenant Key".to_string()),
        }]);

        assert!(!map.contains_key("local-dev-key"));
        let ctx = map.get("tenant-key").expect("tenant key record");
        assert_eq!(ctx.org_id, "org-123");
        assert_eq!(ctx.actor_id.as_deref(), Some("actor-456"));
        assert_eq!(ctx.actor_label.as_deref(), Some("Tenant Key"));
    }

    #[test]
    fn extract_management_token_reads_x_admin_token_first() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-admin-token",
            HeaderValue::from_static("local-dev-admin-token"),
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer something-else"),
        );

        assert_eq!(
            shared_config::extract_management_token(&headers).as_deref(),
            Some("local-dev-admin-token")
        );
    }

    #[test]
    fn load_identity_admin_tokens_rejects_empty_values() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::set_var("IDENTITY_ADMIN_TOKEN", "   ");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::set_var("APP_ENV", "prod");
        }
        assert!(load_identity_admin_tokens().is_empty());
        unsafe {
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn load_identity_admin_tokens_defaults_in_dev() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::set_var("APP_ENV", "dev");
        }
        let tokens = load_identity_admin_tokens();
        let admin = tokens
            .get("local-dev-admin-token")
            .expect("default dev admin token");
        assert_eq!(admin.actor_id, "local-dev-admin");
        assert!(admin_principal_has_scope(admin, "api_keys:manage"));
        unsafe {
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn build_identity_admin_token_map_preserves_scopes() {
        let tokens = build_admin_principal_map(vec![shared_config::ScopedAdminToken {
            token: "ops-token".to_string(),
            actor_id: "ops-user".to_string(),
            actor_label: Some("Ops User".to_string()),
            scopes: vec!["api_keys:manage".to_string(), "audit:read".to_string()],
            org_ids: Vec::new(),
        }]);
        let admin = tokens.get("ops-token").expect("ops token");
        assert!(admin_principal_has_scope(admin, "api_keys:manage"));
        assert!(admin_principal_has_scope(admin, "audit:read"));
        assert!(!admin_principal_has_scope(admin, "billing:write"));
    }

    #[test]
    fn load_identity_admin_tokens_prefers_json_bundle_over_single_token_fallback() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::set_var(
                "IDENTITY_ADMIN_TOKENS_JSON",
                r#"[{"token":"json-manage-token","actor_id":"json-admin","actor_label":"JSON Admin","scopes":["api_keys:manage"]}]"#,
            );
            env::set_var("IDENTITY_ADMIN_TOKEN", "legacy-single-token");
            env::set_var("APP_ENV", "prod");
        }

        let tokens = load_identity_admin_tokens();
        assert!(tokens.contains_key("json-manage-token"));
        assert!(!tokens.contains_key("legacy-single-token"));
        let admin = tokens
            .get("json-manage-token")
            .expect("json bundle admin token");
        assert_eq!(admin.actor_id, "json-admin");
        assert_eq!(admin.actor_label.as_deref(), Some("JSON Admin"));
        assert!(admin_principal_has_scope(admin, "api_keys:manage"));
        assert!(!admin_principal_has_scope(admin, "audit:read"));

        unsafe {
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn authorize_admin_rejects_token_without_required_scope() {
        let state = AppState::new_for_tests_with_admins(
            HashMap::new(),
            HashMap::from([(
                "ops-read-token".to_string(),
                AdminPrincipal {
                    actor_id: "ops-reader".to_string(),
                    actor_label: Some("Ops Reader".to_string()),
                    scopes: vec!["audit:read".to_string()],
                    org_ids: Vec::new(),
                },
            )]),
            None,
            None,
        );
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-token", HeaderValue::from_static("ops-read-token"));

        let err = authorize_admin(&state, &headers, "api_keys:manage").expect_err("scope denial");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(err.1 .0["error"], "admin token lacks required scope");
        assert_eq!(err.1 .0["message"], "api_keys:manage");
    }

    #[test]
    fn authorize_admin_with_any_scope_accepts_read_only_api_key_admin() {
        let state = AppState::new_for_tests_with_admins(
            HashMap::new(),
            HashMap::from([(
                "ops-read-token".to_string(),
                AdminPrincipal {
                    actor_id: "ops-reader".to_string(),
                    actor_label: Some("Ops Reader".to_string()),
                    scopes: vec!["api_keys:read".to_string()],
                    org_ids: Vec::new(),
                },
            )]),
            None,
            None,
        );
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-token", HeaderValue::from_static("ops-read-token"));

        let admin =
            authorize_admin_with_any_scope(&state, &headers, &["api_keys:read", "api_keys:manage"])
                .expect("read-only api key admin should pass");
        assert_eq!(admin.actor_id, "ops-reader");
    }

    #[test]
    fn authorize_admin_with_any_scope_reports_scope_union_on_denial() {
        let state = AppState::new_for_tests_with_admins(
            HashMap::new(),
            HashMap::from([(
                "audit-only-token".to_string(),
                AdminPrincipal {
                    actor_id: "audit-reader".to_string(),
                    actor_label: Some("Audit Reader".to_string()),
                    scopes: vec!["audit:read".to_string()],
                    org_ids: Vec::new(),
                },
            )]),
            None,
            None,
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-admin-token",
            HeaderValue::from_static("audit-only-token"),
        );

        let err =
            authorize_admin_with_any_scope(&state, &headers, &["api_keys:read", "api_keys:manage"])
                .expect_err("scope denial");
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert_eq!(err.1 .0["error"], "admin token lacks required scope");
        assert_eq!(err.1 .0["message"], "api_keys:read | api_keys:manage");
    }

    #[tokio::test]
    async fn best_effort_audit_uses_supplied_trace_id_and_never_injects_raw_key() {
        #[derive(Clone)]
        struct CaptureState {
            events: Arc<Mutex<Vec<AuditEventCreateRequest>>>,
        }

        async fn capture_event(
            State(state): State<CaptureState>,
            Json(req): Json<AuditEventCreateRequest>,
        ) -> impl IntoResponse {
            state.events.lock().await.push(req.clone());
            (
                AxumStatusCode::CREATED,
                Json(serde_json::json!({ "ok": true })),
            )
                .into_response()
        }

        let capture = CaptureState {
            events: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/v1/audit/events", post(capture_event))
            .with_state(capture.clone());

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock audit server");
        let addr = listener.local_addr().expect("mock audit local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("run mock audit server");
        });

        let state = AppState::new_for_tests_with_audit(
            HashMap::new(),
            Some("local-dev-admin-token".to_string()),
            None,
            Some(format!("http://{addr}")),
        );
        let trace_id = Uuid::new_v4();
        let payload = json!({
            "api_key_id": trace_id,
            "key_prefix": "cex_pk_deadbeef",
            "revoked_reason": "rotation"
        });

        best_effort_audit(
            &state,
            trace_id,
            Some("00000000-0000-0000-0000-00000000ce01".to_string()),
            Some("test-admin".to_string()),
            "identity.api_key.revoked",
            payload.clone(),
        )
        .await;

        let events = capture.events.lock().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trace_id, trace_id);
        assert_eq!(
            events[0].org_id.as_deref(),
            Some("00000000-0000-0000-0000-00000000ce01")
        );
        assert_eq!(events[0].event_type, "identity.api_key.revoked");
        assert_eq!(events[0].actor_id.as_deref(), Some("test-admin"));
        assert_eq!(events[0].payload["api_key_id"], trace_id.to_string());
        assert_eq!(events[0].payload["revoked_reason"], "rotation");
        assert!(events[0].payload.get("api_key").is_none());

        handle.abort();
    }
}
