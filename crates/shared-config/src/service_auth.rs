use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
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

    fn authorize(&self, headers: &HeaderMap) -> Result<(), &'static str> {
        if matches!(self.mode, ServiceAuthMode::Off) {
            return Ok(());
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
            Ok(())
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
    if let Err(code) = config.authorize(request.headers()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "internal service authentication failed",
                "code": code,
                "operation": config.operation,
            })),
        )
            .into_response();
    }

    next.run(request).await
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
    if token.as_bytes().len() < MIN_TOKEN_BYTES {
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

    const STRONG_TOKEN: &str = "9f2d37f86cf447a6b78015f4307d05f91dd2dbd65ad94faca1d8aa03c67dff45";

    #[test]
    fn enforce_mode_requires_allowed_caller_token() {
        let config = ServiceAuthConfig::from_values(
            Some("enforce"),
            Some(&format!(r#"{{"gateway-service":"{STRONG_TOKEN}"}}"#)),
            true,
            "execution:create",
            &["gateway-service"],
        )
        .unwrap();
        assert!(matches!(config.mode, ServiceAuthMode::Enforce));
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
        assert!(constant_time_eq(STRONG_TOKEN.as_bytes(), STRONG_TOKEN.as_bytes()));
        assert!(!constant_time_eq(STRONG_TOKEN.as_bytes(), b"different"));
        assert!(!constant_time_eq(b"same-prefix-a", b"same-prefix-b"));
    }
}
