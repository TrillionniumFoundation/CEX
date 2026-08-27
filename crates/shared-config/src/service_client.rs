use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use std::{collections::HashMap, env};

const AUTH_MODE_ENV: &str = "CEX_INTERNAL_SERVICE_AUTH_MODE";
const TOKEN_MAP_ENV: &str = "CEX_INTERNAL_SERVICE_TOKENS_JSON";
const MIN_TOKEN_BYTES: usize = 32;
const SERVICE_ID_HEADER: HeaderName = HeaderName::from_static("x-cex-service-id");
const SERVICE_TOKEN_HEADER: HeaderName = HeaderName::from_static("x-cex-service-token");
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

pub fn build_internal_http_client(
    service_id: &str,
    require_enforce: bool,
) -> Result<Client, String> {
    let mode = parse_mode(env::var(AUTH_MODE_ENV).ok().as_deref())?;
    if require_enforce && !matches!(mode, ServiceAuthMode::Enforce) {
        return Err(format!(
            "{AUTH_MODE_ENV}=enforce is required in production-like profiles"
        ));
    }

    if matches!(mode, ServiceAuthMode::Off) {
        return Ok(Client::new());
    }

    let raw = env::var(TOKEN_MAP_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{TOKEN_MAP_ENV} is required when service auth is enforced"))?;
    let token_map: HashMap<String, String> =
        serde_json::from_str(&raw).map_err(|error| format!("decode {TOKEN_MAP_ENV}: {error}"))?;
    let token = token_map
        .get(service_id)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{TOKEN_MAP_ENV} does not contain caller {service_id}"))?;
    validate_token(service_id, token)?;

    let mut headers = HeaderMap::new();
    headers.insert(
        SERVICE_ID_HEADER,
        HeaderValue::from_str(service_id)
            .map_err(|error| format!("invalid internal service id: {error}"))?,
    );
    headers.insert(
        SERVICE_TOKEN_HEADER,
        HeaderValue::from_str(token)
            .map_err(|error| format!("invalid internal service token: {error}"))?,
    );

    Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|error| format!("build authenticated internal HTTP client: {error}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_supports_off_and_enforce_modes() {
        assert_eq!(parse_mode(None).unwrap(), ServiceAuthMode::Off);
        assert_eq!(parse_mode(Some("disabled")).unwrap(), ServiceAuthMode::Off);
        assert_eq!(
            parse_mode(Some("required")).unwrap(),
            ServiceAuthMode::Enforce
        );
        assert!(parse_mode(Some("maybe")).is_err());
    }

    #[test]
    fn client_token_validation_rejects_placeholders() {
        assert!(validate_token("gateway-service", "short").is_err());
        assert!(validate_token(
            "gateway-service",
            "REPLACE_GATEWAY_INTERNAL_SERVICE_TOKEN_123456789"
        )
        .is_err());
    }
}
