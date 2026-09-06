use axum::{
    http::{HeaderMap, StatusCode},
    Json,
};
use std::{collections::HashMap, env};

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub identity_base_url: String,
    pub ledger_base_url: String,
    pub execution_base_url: String,
    pub audit_base_url: String,
    pub capability_base_url: String,
}

impl GatewayConfig {
    pub fn from_env() -> Self {
        let host = env::var("GATEWAY_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("GATEWAY_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8080);

        let identity_base_url =
            env::var("IDENTITY_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7001".to_string());
        let ledger_base_url =
            env::var("LEDGER_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7002".to_string());
        let execution_base_url =
            env::var("EXECUTION_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7003".to_string());
        let audit_base_url =
            env::var("AUDIT_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7004".to_string());
        let capability_base_url =
            env::var("CAPABILITY_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:7005".to_string());

        Self {
            host,
            port,
            identity_base_url,
            ledger_base_url,
            execution_base_url,
            audit_base_url,
            capability_base_url,
        }
    }

    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct ScopedAdminTokenRecord {
    pub token: String,
    pub actor_id: Option<String>,
    pub actor_label: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub org_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedAdminToken {
    pub token: String,
    pub actor_id: String,
    pub actor_label: Option<String>,
    pub scopes: Vec<String>,
    pub org_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminPrincipal {
    pub actor_id: String,
    pub actor_label: Option<String>,
    pub scopes: Vec<String>,
    pub org_ids: Vec<String>,
}

impl From<ScopedAdminToken> for AdminPrincipal {
    fn from(value: ScopedAdminToken) -> Self {
        Self {
            actor_id: value.actor_id,
            actor_label: value.actor_label,
            scopes: value.scopes,
            org_ids: value.org_ids,
        }
    }
}

pub fn get_trimmed_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn app_env_is_dev() -> bool {
    env::var("APP_ENV")
        .unwrap_or_else(|_| "dev".to_string())
        .eq_ignore_ascii_case("dev")
}

pub fn parse_scoped_admin_token_records(
    raw: &str,
) -> Result<Vec<ScopedAdminTokenRecord>, serde_json::Error> {
    serde_json::from_str(raw)
}

pub fn normalize_scoped_admin_token_records(
    records: Vec<ScopedAdminTokenRecord>,
    default_actor_id: &str,
    _default_actor_label: &str,
) -> Vec<ScopedAdminToken> {
    let mut tokens = Vec::new();

    for record in records {
        let token = record.token.trim();
        if token.is_empty() {
            continue;
        }

        let scopes = record
            .scopes
            .unwrap_or_default()
            .into_iter()
            .map(|scope| scope.trim().to_string())
            .filter(|scope| !scope.is_empty())
            .collect::<Vec<_>>();

        let actor_id = record
            .actor_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_actor_id.to_string());

        let actor_label = record
            .actor_label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let org_ids = record
            .org_ids
            .unwrap_or_default()
            .into_iter()
            .map(|org_id| org_id.trim().to_string())
            .filter(|org_id| !org_id.is_empty())
            .collect::<Vec<_>>();

        tokens.push(ScopedAdminToken {
            token: token.to_string(),
            actor_id,
            actor_label,
            scopes,
            org_ids,
        });
    }

    tokens
}

pub fn build_single_scoped_admin_token(
    token: String,
    default_actor_id: &str,
    default_actor_label: &str,
    default_scopes: &[&str],
) -> ScopedAdminToken {
    ScopedAdminToken {
        token,
        actor_id: default_actor_id.to_string(),
        actor_label: Some(default_actor_label.to_string()),
        scopes: default_scopes
            .iter()
            .map(|scope| scope.to_string())
            .collect(),
        org_ids: Vec::new(),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScopedAdminTokenDevDefault<'a> {
    pub token: &'a str,
    pub actor_id: &'a str,
    pub actor_label: &'a str,
    pub scopes: &'a [&'a str],
}

#[derive(Debug, Clone, Copy)]
pub struct ScopedAdminTokenEnvConfig<'a> {
    pub bundle_env_names: &'a [&'a str],
    pub bundle_default_actor_id: &'a str,
    pub bundle_default_actor_label: &'a str,
    pub single_env_names: &'a [&'a str],
    pub single_default_actor_id: &'a str,
    pub single_default_actor_label: &'a str,
    pub single_default_scopes: &'a [&'a str],
    pub dev_default: Option<ScopedAdminTokenDevDefault<'a>>,
}

pub fn load_scoped_admin_tokens_from_env(
    config: ScopedAdminTokenEnvConfig<'_>,
) -> Vec<ScopedAdminToken> {
    for env_name in config.bundle_env_names {
        if let Some(raw) = get_trimmed_env(env_name) {
            if let Ok(records) = parse_scoped_admin_token_records(&raw) {
                let tokens = normalize_scoped_admin_token_records(
                    records,
                    config.bundle_default_actor_id,
                    config.bundle_default_actor_label,
                );
                if !tokens.is_empty() {
                    return tokens;
                }
            }
        }
    }

    for env_name in config.single_env_names {
        if let Some(value) = get_trimmed_env(env_name) {
            return vec![build_single_scoped_admin_token(
                value,
                config.single_default_actor_id,
                config.single_default_actor_label,
                config.single_default_scopes,
            )];
        }
    }

    if app_env_is_dev() {
        if let Some(default) = config.dev_default {
            return vec![build_single_scoped_admin_token(
                default.token.to_string(),
                default.actor_id,
                default.actor_label,
                default.scopes,
            )];
        }
    }

    Vec::new()
}

pub fn load_identity_scoped_admin_tokens() -> Vec<ScopedAdminToken> {
    load_scoped_admin_tokens_from_env(ScopedAdminTokenEnvConfig {
        bundle_env_names: &["IDENTITY_ADMIN_TOKENS_JSON"],
        bundle_default_actor_id: "identity-admin",
        bundle_default_actor_label: "Identity Admin",
        single_env_names: &["IDENTITY_ADMIN_TOKEN"],
        single_default_actor_id: "identity-admin",
        single_default_actor_label: "Identity Admin",
        single_default_scopes: &["api_keys:manage", "audit:read"],
        dev_default: Some(ScopedAdminTokenDevDefault {
            token: "local-dev-admin-token",
            actor_id: "local-dev-admin",
            actor_label: "Local Dev Admin",
            scopes: &["api_keys:manage", "audit:read"],
        }),
    })
}

pub fn load_audit_scoped_admin_tokens() -> Vec<ScopedAdminToken> {
    load_scoped_admin_tokens_from_env(ScopedAdminTokenEnvConfig {
        bundle_env_names: &["AUDIT_ADMIN_TOKENS_JSON", "IDENTITY_ADMIN_TOKENS_JSON"],
        bundle_default_actor_id: "audit-admin",
        bundle_default_actor_label: "Audit Admin",
        single_env_names: &["AUDIT_ADMIN_TOKEN", "IDENTITY_ADMIN_TOKEN"],
        single_default_actor_id: "local-dev-admin",
        single_default_actor_label: "Local Dev Admin",
        single_default_scopes: &["api_keys:manage", "audit:read"],
        dev_default: Some(ScopedAdminTokenDevDefault {
            token: "local-dev-admin-token",
            actor_id: "local-dev-admin",
            actor_label: "Local Dev Admin",
            scopes: &["api_keys:manage", "audit:read"],
        }),
    })
}

pub fn load_execution_scoped_admin_tokens() -> Vec<ScopedAdminToken> {
    load_scoped_admin_tokens_from_env(ScopedAdminTokenEnvConfig {
        bundle_env_names: &["EXECUTION_ADMIN_TOKENS_JSON", "IDENTITY_ADMIN_TOKENS_JSON"],
        bundle_default_actor_id: "execution-admin",
        bundle_default_actor_label: "Execution Admin",
        single_env_names: &["EXECUTION_ADMIN_TOKEN", "IDENTITY_ADMIN_TOKEN"],
        single_default_actor_id: "local-dev-admin",
        single_default_actor_label: "Local Dev Admin",
        single_default_scopes: &["executions:manage", "executions:read"],
        dev_default: Some(ScopedAdminTokenDevDefault {
            token: "local-dev-admin-token",
            actor_id: "local-dev-admin",
            actor_label: "Local Dev Admin",
            scopes: &["executions:manage", "executions:read"],
        }),
    })
}

pub fn load_ledger_scoped_admin_tokens() -> Vec<ScopedAdminToken> {
    load_scoped_admin_tokens_from_env(ScopedAdminTokenEnvConfig {
        bundle_env_names: &["LEDGER_ADMIN_TOKENS_JSON", "IDENTITY_ADMIN_TOKENS_JSON"],
        bundle_default_actor_id: "ledger-admin",
        bundle_default_actor_label: "Ledger Admin",
        single_env_names: &["LEDGER_ADMIN_TOKEN", "IDENTITY_ADMIN_TOKEN"],
        single_default_actor_id: "local-dev-admin",
        single_default_actor_label: "Local Dev Admin",
        single_default_scopes: &["ledger:manage", "ledger:read"],
        dev_default: Some(ScopedAdminTokenDevDefault {
            token: "local-dev-admin-token",
            actor_id: "local-dev-admin",
            actor_label: "Local Dev Admin",
            scopes: &["ledger:manage", "ledger:read"],
        }),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminAuthorizationFailure {
    NotConfigured {
        error: String,
        message: Option<String>,
    },
    MissingToken,
    InvalidToken,
    MissingScope {
        required_scopes: Vec<String>,
    },
}

impl AdminAuthorizationFailure {
    pub fn into_response(self) -> (StatusCode, Json<serde_json::Value>) {
        match self {
            Self::NotConfigured { error, message } => {
                let payload = match message {
                    Some(message) => serde_json::json!({ "error": error, "message": message }),
                    None => serde_json::json!({ "error": error }),
                };
                (StatusCode::SERVICE_UNAVAILABLE, Json(payload))
            }
            Self::MissingToken => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "missing admin token" })),
            ),
            Self::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "invalid admin token" })),
            ),
            Self::MissingScope { required_scopes } => (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "admin token lacks required scope",
                    "message": required_scopes.join(" | "),
                })),
            ),
        }
    }
}

pub fn authorize_scoped_admin_from_map<'a, T, F>(
    headers: &HeaderMap,
    admins: &'a HashMap<String, T>,
    required_scopes: &[&str],
    has_scope: F,
    not_configured_error: &str,
    not_configured_message: Option<&str>,
) -> Result<&'a T, AdminAuthorizationFailure>
where
    F: Fn(&T, &str) -> bool,
{
    if admins.is_empty() {
        return Err(AdminAuthorizationFailure::NotConfigured {
            error: not_configured_error.to_string(),
            message: not_configured_message.map(str::to_string),
        });
    }

    let Some(provided) = extract_management_token(headers) else {
        return Err(AdminAuthorizationFailure::MissingToken);
    };

    let Some(admin) = admins.get(&provided) else {
        return Err(AdminAuthorizationFailure::InvalidToken);
    };

    if !required_scopes
        .iter()
        .any(|required_scope| has_scope(admin, required_scope))
    {
        return Err(AdminAuthorizationFailure::MissingScope {
            required_scopes: required_scopes
                .iter()
                .map(|scope| scope.to_string())
                .collect(),
        });
    }

    Ok(admin)
}

pub fn build_admin_principal_map(
    records: Vec<ScopedAdminToken>,
) -> HashMap<String, AdminPrincipal> {
    let mut map = HashMap::new();

    for record in records {
        let token = record.token.clone();
        map.insert(token, record.into());
    }

    map
}

pub fn extract_management_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value.to_string());
    }

    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn admin_principal_has_scope(admin: &AdminPrincipal, required_scope: &str) -> bool {
    admin.scopes.iter().any(|scope| scope == required_scope)
}

pub fn admin_principal_allows_org(admin: &AdminPrincipal, org_id: &str) -> bool {
    admin.org_ids.is_empty()
        || admin
            .org_ids
            .iter()
            .any(|allowed_org| allowed_org.eq_ignore_ascii_case(org_id))
}

pub fn token_has_scope(token: &ScopedAdminToken, required_scope: &str) -> bool {
    token.scopes.iter().any(|scope| scope == required_scope)
}

pub fn select_token_by_scope_priority<'a>(
    tokens: &'a [ScopedAdminToken],
    required_scopes: &[&str],
) -> Option<&'a ScopedAdminToken> {
    for required_scope in required_scopes {
        if let Some(token) = tokens
            .iter()
            .find(|token| token_has_scope(token, required_scope))
        {
            return Some(token);
        }
    }

    None
}

pub fn select_identity_manage_token(tokens: &[ScopedAdminToken]) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["api_keys:manage"])
}

pub fn select_identity_read_or_manage_token(
    tokens: &[ScopedAdminToken],
) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["api_keys:read", "api_keys:manage"])
}

pub fn select_audit_read_token(tokens: &[ScopedAdminToken]) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["audit:read"])
}

pub fn select_execution_manage_token(tokens: &[ScopedAdminToken]) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["executions:manage"])
}

pub fn select_execution_read_or_manage_token(
    tokens: &[ScopedAdminToken],
) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["executions:read", "executions:manage"])
}

pub fn select_ledger_manage_token(tokens: &[ScopedAdminToken]) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["ledger:manage"])
}

pub fn select_ledger_read_or_manage_token(
    tokens: &[ScopedAdminToken],
) -> Option<&ScopedAdminToken> {
    select_token_by_scope_priority(tokens, &["ledger:read", "ledger:manage"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn normalize_scoped_admin_token_records_preserves_actor_metadata_and_scopes() {
        let tokens = normalize_scoped_admin_token_records(
            vec![ScopedAdminTokenRecord {
                token: "ops-token".to_string(),
                actor_id: Some("ops-admin".to_string()),
                actor_label: Some("Ops Admin".to_string()),
                scopes: Some(vec![
                    "api_keys:manage".to_string(),
                    "audit:read".to_string(),
                ]),
                org_ids: None,
            }],
            "default-admin",
            "Default Admin",
        );

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token, "ops-token");
        assert_eq!(tokens[0].actor_id, "ops-admin");
        assert_eq!(tokens[0].actor_label.as_deref(), Some("Ops Admin"));
        assert!(token_has_scope(&tokens[0], "api_keys:manage"));
        assert!(token_has_scope(&tokens[0], "audit:read"));
        assert!(tokens[0].org_ids.is_empty());
    }

    #[test]
    fn select_token_by_scope_priority_prefers_earlier_scope() {
        let tokens = vec![
            build_single_scoped_admin_token(
                "manage-token".to_string(),
                "identity-admin",
                "Identity Admin",
                &["api_keys:manage"],
            ),
            build_single_scoped_admin_token(
                "read-token".to_string(),
                "identity-reader",
                "Identity Reader",
                &["api_keys:read"],
            ),
        ];

        let selected =
            select_token_by_scope_priority(&tokens, &["api_keys:read", "api_keys:manage"])
                .expect("selected token");
        assert_eq!(selected.token, "read-token");
    }

    #[test]
    fn app_env_is_dev_defaults_true_when_unset() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::remove_var("APP_ENV");
        }

        assert!(app_env_is_dev());
    }

    #[test]
    fn extract_management_token_prefers_x_admin_token_then_bearer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer bearer-token"),
        );
        headers.insert("x-admin-token", HeaderValue::from_static("header-token"));

        assert_eq!(
            extract_management_token(&headers).as_deref(),
            Some("header-token")
        );

        headers.remove("x-admin-token");
        assert_eq!(
            extract_management_token(&headers).as_deref(),
            Some("bearer-token")
        );
    }

    #[test]
    fn build_admin_principal_map_preserves_actor_metadata_and_scopes() {
        let map = build_admin_principal_map(vec![ScopedAdminToken {
            token: "ops-token".to_string(),
            actor_id: "ops-admin".to_string(),
            actor_label: Some("Ops Admin".to_string()),
            scopes: vec!["api_keys:manage".to_string(), "audit:read".to_string()],
            org_ids: vec!["00000000-0000-0000-0000-00000000ce01".to_string()],
        }]);

        let admin = map.get("ops-token").expect("ops token");
        assert_eq!(admin.actor_id, "ops-admin");
        assert_eq!(admin.actor_label.as_deref(), Some("Ops Admin"));
        assert!(admin_principal_has_scope(admin, "api_keys:manage"));
        assert!(admin_principal_has_scope(admin, "audit:read"));
        assert!(admin_principal_allows_org(
            admin,
            "00000000-0000-0000-0000-00000000ce01"
        ));
        assert!(!admin_principal_allows_org(
            admin,
            "00000000-0000-0000-0000-00000000ce02"
        ));
    }

    #[test]
    fn authorize_scoped_admin_from_map_reports_missing_scope_union() {
        let mut headers = HeaderMap::new();
        headers.insert("x-admin-token", HeaderValue::from_static("ops-token"));

        let admins = HashMap::from([(
            "ops-token".to_string(),
            ScopedAdminToken {
                token: "ops-token".to_string(),
                actor_id: "ops-admin".to_string(),
                actor_label: Some("Ops Admin".to_string()),
                scopes: vec!["audit:read".to_string()],
                org_ids: Vec::new(),
            },
        )]);

        let err = authorize_scoped_admin_from_map(
            &headers,
            &admins,
            &["api_keys:read", "api_keys:manage"],
            token_has_scope,
            "identity admin token not configured",
            Some("set identity admin env"),
        )
        .expect_err("scope denial");

        assert_eq!(
            err,
            AdminAuthorizationFailure::MissingScope {
                required_scopes: vec!["api_keys:read".to_string(), "api_keys:manage".to_string()],
            }
        );
        let response = err.into_response();
        assert_eq!(response.0, StatusCode::FORBIDDEN);
        assert_eq!(response.1 .0["message"], "api_keys:read | api_keys:manage");
    }

    #[test]
    fn load_execution_scoped_admin_tokens_prefers_execution_bundle_over_identity_bundle() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::set_var(
                "EXECUTION_ADMIN_TOKENS_JSON",
                r#"[{"token":"execution-manage-token","scopes":["executions:manage"]}]"#,
            );
            env::set_var(
                "IDENTITY_ADMIN_TOKENS_JSON",
                r#"[{"token":"identity-manage-token","scopes":["api_keys:manage"]}]"#,
            );
            env::remove_var("EXECUTION_ADMIN_TOKEN");
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::set_var("APP_ENV", "prod");
        }

        let tokens = load_execution_scoped_admin_tokens();
        let selected = select_execution_manage_token(&tokens).expect("execution manage token");
        assert_eq!(selected.token, "execution-manage-token");

        unsafe {
            env::remove_var("EXECUTION_ADMIN_TOKENS_JSON");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn select_execution_read_or_manage_token_prefers_read_scope_over_manage_scope() {
        let tokens = vec![
            build_single_scoped_admin_token(
                "execution-manage-token".to_string(),
                "execution-admin",
                "Execution Admin",
                &["executions:manage"],
            ),
            build_single_scoped_admin_token(
                "execution-read-token".to_string(),
                "execution-reader",
                "Execution Reader",
                &["executions:read"],
            ),
        ];

        let selected =
            select_execution_read_or_manage_token(&tokens).expect("selected execution token");
        assert_eq!(selected.token, "execution-read-token");
    }

    #[test]
    fn load_ledger_scoped_admin_tokens_prefers_ledger_bundle_over_identity_bundle() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe {
            env::set_var(
                "LEDGER_ADMIN_TOKENS_JSON",
                r#"[{"token":"ledger-manage-token","scopes":["ledger:manage"]}]"#,
            );
            env::set_var(
                "IDENTITY_ADMIN_TOKENS_JSON",
                r#"[{"token":"identity-manage-token","scopes":["api_keys:manage"]}]"#,
            );
            env::remove_var("LEDGER_ADMIN_TOKEN");
            env::remove_var("IDENTITY_ADMIN_TOKEN");
            env::set_var("APP_ENV", "prod");
        }

        let tokens = load_ledger_scoped_admin_tokens();
        let selected = select_ledger_manage_token(&tokens).expect("ledger manage token");
        assert_eq!(selected.token, "ledger-manage-token");

        unsafe {
            env::remove_var("LEDGER_ADMIN_TOKENS_JSON");
            env::remove_var("IDENTITY_ADMIN_TOKENS_JSON");
            env::remove_var("APP_ENV");
        }
    }

    #[test]
    fn select_ledger_read_or_manage_token_prefers_read_scope_over_manage_scope() {
        let tokens = vec![
            build_single_scoped_admin_token(
                "ledger-manage-token".to_string(),
                "ledger-admin",
                "Ledger Admin",
                &["ledger:manage"],
            ),
            build_single_scoped_admin_token(
                "ledger-read-token".to_string(),
                "ledger-reader",
                "Ledger Reader",
                &["ledger:read"],
            ),
        ];

        let selected = select_ledger_read_or_manage_token(&tokens).expect("selected ledger token");
        assert_eq!(selected.token, "ledger-read-token");
    }
}
