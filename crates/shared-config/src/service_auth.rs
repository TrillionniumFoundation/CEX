use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::{collections::HashMap, env, sync::Arc};

const SERVICE_ID_HEADER: &str = "x-cex-service-id";
const SERVICE_TOKEN_HEADER: &str = "x-cex-service-token";
const TOKEN_MAP_ENV: &str = "CEX_INTERNAL_SERVICE_TOKENS_JSON";
const AUTH_MODE_ENV: &str = "CEX_INTERNAL_SERVICE_AUTH_MODE";
const MIN_TOKEN_BYTES: usize = 32;
const WEAK_MARKERS: &[&str] = &[
    "local-dev",
    "change-me",
    "changeme",
    "replace-me",
    "replace_",
    "insecure-default",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceAuthMode {
    Off,
    Enforce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedService {
    pub service_id: String,
    pub authenticated: bool,
}

#[derive(Clone)]
pub struct ServiceAuthConfig {
    mode: ServiceAuthMode,
    operation: &'static str,
    allowed_tokens: Arc<HashMap<String, String>>,
}

impl ServiceAuthConfig {
    pub fn execution_create_from_env(require_enforce: bool) -> Result<Self, String> {
        Self::from_values(
            env::var(AUTH_MODE_ENV).ok().as_deref(),
            env::var(TOKEN_MAP_ENV).ok().as_deref(),
            require_enforce,
            "execution:create",
            &["gateway-service"],
        )
    }

    pub fn identity_resolve_from_env(require_enforce: bool) -> Result<Self, String> {
        Self::from_values(
            env::var(AUTH_MODE_ENV).ok().as_deref(),
            env::var(TOKEN_MAP_ENV).ok().as_deref(),
            require_enforce,
            "identity:resolve",
            &["gateway-service"],
        )
    }

    pub fn audit_write_from_env(require_enforce: bool) -> Result<Self, String> {
        Self::from_values(
            env::var(AUTH_MODE_ENV).ok().as_deref(),
            env::var(TOKEN_MAP_ENV).ok().as_deref(),
            require_enforce,
            "audit:write",
            &[
                "gateway-service",
                "identity-service",
                "execution-service",
                "audit-outbox-dispatcher",
            ],
        )
    }

    fn from_values(
        mode_raw: Option<&str>,
        token_map_raw: Option<&str>,
        require_enforce: bool,
        operation: &'static str,
        allowed_callers: &[&str],
    ) -> Result<Self, String> {
        let mode = parse_mode(mode_raw)?;
        if require_enforce && !matches!(mode, ServiceAuthMode::Enforce) {
            return Err(format!(
                "{AUTH_MODE_ENV}=enforce is required for operation {operation}"
            ));
        }

        if matches!(mode, ServiceAuthMode::Off) {
            return Ok(Self {
                mode,
                operation,
                allowed_tokens: Arc::new(HashMap::new()),
            });
        }

        let raw = token_map_raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{TOKEN_MAP_ENV} is required when service auth is enforced"))?;
        let token_map: HashMap<String, String> = serde_json::from_str(raw)
            .map_err(|error| format!("decode {TOKEN_MAP_ENV}: {error}"))?;

        let mut allowed_tokens = HashMap::new();
        for caller in allowed_callers {
            let token = token_map
                .get(*caller)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!(
                        "{TOKEN_MAP_ENV} must contain a non-empty token for caller {caller}"
                    )
                })?;
            validate_token(caller, token)?;
            allowed_tokens.insert((*caller).to_string(), token.to_string());
        }

        Ok(Self {
            mode,
            operation,
            allowed_tokens: Arc::new(allowed_tokens),
        })
    }

    fn authenticate(
        &self,
        headers: &HeaderMap,
    ) -> Result<AuthenticatedService, &'static str> {
        if matches!(self.mode, ServiceAuthMode::Off) {
            return Ok(AuthenticatedService {
                service_id: "compatibility-unauthenticated".to_string(),
                authenticated: false,
            });
        }

        let service_id = headers
            .get(SERVICE_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("missing_service_identity")?;
        let supplied_token = headers
            .get(SERVICE_TOKEN_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or("missing_service_token")?;
        let expected_token = self
            .allowed_tokens
            .get(service_id)
            .ok_or("service_identity_not_allowed")?;

        if constant_time_eq(expected_token.as_bytes(), supplied_token.as_bytes()) {
            Ok(AuthenticatedService {
                service_id: service_id.to_string(),
                authenticated: true,
            })
        } else {
            Err("invalid_service_token")
        }
    }
}

pub async fn require_service_auth(
    State(config): State<Arc<ServiceAuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    authenticate_and_continue(&config, request, next).await
}

pub async fn require_identity_resolve_auth(
    State(config): State<Arc<ServiceAuthConfig>>,
    request: Request,
    next: Next,
) -> Response {
    if request.method() == Method::POST && request.uri().path() == "/v1/auth/resolve" {
        authenticate_and_continue(&config, request, next).await
    } else {
        next.run(request).await
    }
}

async fn authenticate_and_continue(
    config: &ServiceAuthConfig,
    mut request: Request,
    next: Next,
) -> Response {
    let principal = match config.authenticate(request.headers()) {
        Ok(principal) => principal,
        Err(code) => return auth_failure(config.operation, code),
    };

    request.extensions_mut().insert(principal);
    next.run(request).await
}

fn auth_failure(operation: &'static str, code: &'static str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "error": "internal service authentication failed",
            "code": code,
            "operation": operation,
        })),
    )
        .into_response()
}

fn parse_mode(raw: Option<&str>) -> Result<ServiceAuthMode, String> {
    match raw.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(ServiceAuthMode::Off),
        Some(value) => match value.to_ascii_lowercase().as_str() {
            "off" | "disabled" => Ok(ServiceAuthMode::Off),
            "enforce" | "required" => Ok(ServiceAuthMode::Enforce),
            _ => Err(format!(
                "{AUTH_MODE_ENV} must be off/disabled or enforce/required"
            )),
        },
    }
}

fn validate_token(service_id: &str, token: &str) -> Result<(), String> {
    if token.len() < MIN_TOKEN_BYTES {
        return Err(format!(
            "internal token for {service_id} must be at least {MIN_TOKEN_BYTES} bytes"
        ));
    }

    let lowered = token.to_ascii_lowercase();
    if let Some(marker) = WEAK_MARKERS
        .iter()
        .copied()
        .find(|marker| lowered.contains(marker))
    {
        return Err(format!(
            "internal token for {service_id} contains forbidden marker '{marker}'"
        ));
    }

    Ok(())
}

fn constant_time_eq(expected: &[u8], supplied: &[u8]) -> bool {
    let max_len = expected.len().max(supplied.len());
    let mut difference = expected.len() ^ supplied.len();

    for index in 0..max_len {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = supplied.get(index).copied().unwrap_or_default();
        difference |= usize::from(left ^ right);
    }

    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const GATEWAY_TOKEN: &str =
        "9f2d37f86cf447a6b78015f4307d05f91dd2dbd65ad94faca1d8aa03c67dff45";
    const IDENTITY_TOKEN: &str =
        "a34eec99e9034dcc94ef660136ebdd3719978e3dd9874312ab7b7c2b9b14ea81";
    const EXECUTION_TOKEN: &str =
        "ed34683df2c7423d81a5e8db2267463fa0246fd2970741f0b104c21f9f32f4fb";
    const DISPATCHER_TOKEN: &str =
        "7c80ec8780d94989843c24824f46892c48fcdb23c3d848a38f3211fa2ad2871a";

    #[test]
    fn enforce_mode_requires_allowed_caller_token() {
        let token_map = format!(r#"{{"gateway-service":"{GATEWAY_TOKEN}"}}"#);
        let config = ServiceAuthConfig::from_values(
            Some("enforce"),
            Some(&token_map),
            true,
            "execution:create",
            &["gateway-service"],
        )
        .unwrap();
        assert!(matches!(config.mode, ServiceAuthMode::Enforce));
    }

    #[test]
    fn identity_resolve_only_requires_gateway_identity() {
        let token_map = format!(r#"{{"gateway-service":"{GATEWAY_TOKEN}"}}"#);
        let config = ServiceAuthConfig::from_values(
            Some("enforce"),
            Some(&token_map),
            true,
            "identity:resolve",
            &["gateway-service"],
        )
        .unwrap();
        assert_eq!(config.allowed_tokens.len(), 1);
        assert!(config.allowed_tokens.contains_key("gateway-service"));
    }

    #[test]
    fn audit_writer_config_includes_the_dispatcher() {
        let token_map = format!(
            r#"{{"gateway-service":"{GATEWAY_TOKEN}","identity-service":"{IDENTITY_TOKEN}","execution-service":"{EXECUTION_TOKEN}","audit-outbox-dispatcher":"{DISPATCHER_TOKEN}"}}"#
        );
        let config = ServiceAuthConfig::from_values(
            Some("enforce"),
            Some(&token_map),
            true,
            "audit:write",
            &[
                "gateway-service",
                "identity-service",
                "execution-service",
                "audit-outbox-dispatcher",
            ],
        )
        .unwrap();
        assert_eq!(config.allowed_tokens.len(), 4);
        assert!(config
            .allowed_tokens
            .contains_key("audit-outbox-dispatcher"));
    }

    #[test]
    fn production_requirement_rejects_off_mode() {
        let error = ServiceAuthConfig::from_values(
            Some("off"),
            None,
            true,
            "execution:create",
            &["gateway-service"],
        )
        .err()
        .unwrap();
        assert!(error.contains("=enforce"));
    }

    #[test]
    fn compatibility_mode_yields_untrusted_principal() {
        let config = ServiceAuthConfig::from_values(
            Some("off"),
            None,
            false,
            "audit:write",
            &["gateway-service"],
        )
        .unwrap();
        let principal = config.authenticate(&HeaderMap::new()).unwrap();
        assert!(!principal.authenticated);
        assert_eq!(principal.service_id, "compatibility-unauthenticated");
    }

    #[test]
    fn weak_and_short_tokens_are_rejected() {
        assert!(validate_token("gateway-service", "short").is_err());
        assert!(validate_token(
            "gateway-service",
            "replace_me_with_a_real_gateway_token_123456789"
        )
        .is_err());
    }

    #[test]
    fn token_comparison_handles_length_and_value_mismatch() {
        assert!(constant_time_eq(
            GATEWAY_TOKEN.as_bytes(),
            GATEWAY_TOKEN.as_bytes()
        ));
        assert!(!constant_time_eq(GATEWAY_TOKEN.as_bytes(), b"different"));
        assert!(!constant_time_eq(b"same-prefix-a", b"same-prefix-b"));
    }
}
