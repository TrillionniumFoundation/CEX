use capability_service::AppState;
use shared_types::CapabilityRecord;
use std::{collections::HashSet, env, fmt, net::SocketAddr};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:7005";
const DEFAULT_MAX_REGISTRY_RECORDS: usize = 10_000;
const MAX_REGISTRY_JSON_BYTES: usize = 2 * 1024 * 1024;
const OPENCLAW_DISCOVERY_ENV: &[&str] = &[
    "CAPABILITY_OPENCLAW_MODELS_JSON_PATH",
    "OPENCLAW_MODELS_JSON_PATH",
    "OPENCLAW_AGENT_DIR",
    "OPENCLAW_CLI_BIN",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfile {
    Test,
    Local,
    Dev,
    Beta,
    Staging,
    Production,
}

impl RuntimeProfile {
    fn parse(raw: &str) -> Result<Self, StartupError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" => Ok(Self::Test),
            "local" | "local_dev" | "local-dev" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "staging" | "stage" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            other => Err(StartupError::new(
                "invalid_runtime_profile",
                format!(
                    "unsupported capability-service runtime profile '{other}'; \
                     expected test, local, dev, beta, staging, or production"
                ),
            )),
        }
    }

    fn is_production_like(self) -> bool {
        matches!(self, Self::Beta | Self::Staging | Self::Production)
    }
}

impl fmt::Display for RuntimeProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Test => "test",
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Staging => "staging",
            Self::Production => "production",
        })
    }
}

#[derive(Debug)]
pub struct StartupError {
    code: &'static str,
    message: String,
}

impl StartupError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for StartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

pub struct RuntimeConfig {
    profile: RuntimeProfile,
    bind_addr: SocketAddr,
    strict_records: Option<Vec<CapabilityRecord>>,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, StartupError> {
        let cex_profile = trimmed_env("CEX_RUNTIME_PROFILE");
        let app_env = trimmed_env("APP_ENV");
        let allow_implicit_dev = trimmed_env("CEX_ALLOW_IMPLICIT_DEV_PROFILE")
            .map(|raw| parse_bool("CEX_ALLOW_IMPLICIT_DEV_PROFILE", &raw))
            .transpose()?
            .unwrap_or(false);
        let profile = resolve_profile_values(
            cex_profile.as_deref(),
            app_env.as_deref(),
            allow_implicit_dev,
        )?;

        let bind_raw =
            trimmed_env("CAPABILITY_BIND_ADDR").unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string());
        let bind_addr = bind_raw.parse::<SocketAddr>().map_err(|error| {
            StartupError::new(
                "invalid_bind_address",
                format!("CAPABILITY_BIND_ADDR must be a socket address: {error}"),
            )
        })?;

        let strict_records = if profile.is_production_like() {
            reject_openclaw_discovery()?;
            let max_records =
                parse_max_records(trimmed_env("CAPABILITY_MAX_REGISTRY_RECORDS").as_deref())?;
            let raw = trimmed_env("CAPABILITY_STATIC_REGISTRY_JSON").ok_or_else(|| {
                StartupError::new(
                    "registry_required",
                    "CAPABILITY_STATIC_REGISTRY_JSON is required in production-like profiles",
                )
            })?;
            Some(validate_strict_registry(&raw, max_records)?)
        } else {
            None
        };

        Ok(Self {
            profile,
            bind_addr,
            strict_records,
        })
    }

    pub fn profile(&self) -> RuntimeProfile {
        self.profile
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn registry_source(&self) -> &'static str {
        if self.strict_records.is_some() {
            "validated_static"
        } else {
            "development_discovery"
        }
    }

    pub async fn build_state(&self) -> AppState {
        match &self.strict_records {
            Some(records) => AppState::new_for_tests(records.clone()),
            None => AppState::from_env().await,
        }
    }
}

fn resolve_profile_values(
    cex_profile: Option<&str>,
    app_env: Option<&str>,
    allow_implicit_dev: bool,
) -> Result<RuntimeProfile, StartupError> {
    let primary = cex_profile.map(RuntimeProfile::parse).transpose()?;
    let compatibility = app_env.map(RuntimeProfile::parse).transpose()?;

    match (primary, compatibility) {
        (Some(left), Some(right)) if left != right => Err(StartupError::new(
            "runtime_profile_conflict",
            format!("CEX_RUNTIME_PROFILE resolves to {left}, but APP_ENV resolves to {right}"),
        )),
        (Some(profile), _) | (_, Some(profile)) => Ok(profile),
        (None, None) if allow_implicit_dev => Ok(RuntimeProfile::Dev),
        (None, None) => Err(StartupError::new(
            "runtime_profile_missing",
            "set CEX_RUNTIME_PROFILE or APP_ENV explicitly; implicit dev is disabled",
        )),
    }
}

fn reject_openclaw_discovery() -> Result<(), StartupError> {
    let configured = OPENCLAW_DISCOVERY_ENV
        .iter()
        .copied()
        .filter(|name| trimmed_env(name).is_some())
        .collect::<Vec<_>>();
    if configured.is_empty() {
        Ok(())
    } else {
        Err(StartupError::new(
            "development_discovery_forbidden",
            format!(
                "production-like capability-service forbids local OpenClaw discovery variables: {}",
                configured.join(", ")
            ),
        ))
    }
}

fn parse_max_records(raw: Option<&str>) -> Result<usize, StartupError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_MAX_REGISTRY_RECORDS);
    };
    let value = raw.parse::<usize>().map_err(|_| {
        StartupError::new(
            "invalid_registry_limit",
            "CAPABILITY_MAX_REGISTRY_RECORDS must be a positive integer",
        )
    })?;
    if !(1..=100_000).contains(&value) {
        return Err(StartupError::new(
            "invalid_registry_limit",
            "CAPABILITY_MAX_REGISTRY_RECORDS must be between 1 and 100000",
        ));
    }
    Ok(value)
}

fn validate_strict_registry(
    raw: &str,
    max_records: usize,
) -> Result<Vec<CapabilityRecord>, StartupError> {
    if raw.len() > MAX_REGISTRY_JSON_BYTES {
        return Err(StartupError::new(
            "registry_too_large",
            format!("CAPABILITY_STATIC_REGISTRY_JSON exceeds {MAX_REGISTRY_JSON_BYTES} bytes"),
        ));
    }

    let records = serde_json::from_str::<Vec<CapabilityRecord>>(raw).map_err(|error| {
        StartupError::new(
            "invalid_registry_json",
            format!("CAPABILITY_STATIC_REGISTRY_JSON is invalid: {error}"),
        )
    })?;
    if records.is_empty() {
        return Err(StartupError::new(
            "empty_registry",
            "production-like capability registry must contain at least one record",
        ));
    }
    if records.len() > max_records {
        return Err(StartupError::new(
            "registry_record_limit",
            format!(
                "capability registry contains {} records, limit is {max_records}",
                records.len()
            ),
        ));
    }

    let mut identifiers = HashSet::with_capacity(records.len());
    let mut enabled = 0usize;
    for (index, record) in records.iter().enumerate() {
        for (label, value) in [
            ("capability_id", record.capability_id.as_str()),
            ("kind", record.kind.as_str()),
            ("provider", record.provider.as_str()),
            ("provider_ref", record.provider_ref.as_str()),
            ("display_name", record.display_name.as_str()),
            ("version", record.version.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(StartupError::new(
                    "invalid_registry_record",
                    format!("record {index} has an empty {label}"),
                ));
            }
        }

        if !identifiers.insert(record.capability_id.as_str()) {
            return Err(StartupError::new(
                "duplicate_capability_id",
                format!("duplicate capability_id {}", record.capability_id),
            ));
        }

        let authority_fields = format!(
            "{} {} {} {}",
            record.capability_id, record.provider, record.provider_ref, record.version
        )
        .to_ascii_lowercase();
        if ["demo", "placeholder", "change-me", "local-development"]
            .iter()
            .any(|marker| authority_fields.contains(marker))
        {
            return Err(StartupError::new(
                "development_capability_forbidden",
                format!(
                    "record {} contains a development/placeholder authority marker",
                    record.capability_id
                ),
            ));
        }

        if record.version.eq_ignore_ascii_case("openclaw-local") {
            return Err(StartupError::new(
                "development_capability_forbidden",
                format!(
                    "record {} uses development-only openclaw-local version",
                    record.capability_id
                ),
            ));
        }

        if record.enabled {
            enabled += 1;
        }
    }

    if enabled == 0 {
        return Err(StartupError::new(
            "no_enabled_capability",
            "production-like registry must contain at least one enabled capability",
        ));
    }

    Ok(records)
}

fn trimmed_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_bool(name: &str, raw: &str) -> Result<bool, StartupError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(StartupError::new(
            "invalid_boolean_configuration",
            format!("{name} must be true/false, 1/0, yes/no, or on/off"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_profile_values, validate_strict_registry, RuntimeProfile};

    fn record(id: &str, provider: &str, enabled: bool) -> serde_json::Value {
        serde_json::json!({
            "capability_id": id,
            "kind": "model",
            "provider": provider,
            "provider_ref": format!("{provider}/model-v1"),
            "display_name": "Production Model",
            "version": "v1",
            "description": null,
            "enabled": enabled
        })
    }

    #[test]
    fn profile_is_explicit_and_conflicts_fail_closed() {
        assert_eq!(
            resolve_profile_values(Some("prod"), Some("production"), false).unwrap(),
            RuntimeProfile::Production
        );
        assert!(resolve_profile_values(Some("production"), Some("dev"), false).is_err());
        assert!(resolve_profile_values(None, None, false).is_err());
        assert_eq!(
            resolve_profile_values(None, None, true).unwrap(),
            RuntimeProfile::Dev
        );
    }

    #[test]
    fn strict_registry_rejects_demo_and_duplicate_authority() {
        let demo = serde_json::to_string(&vec![record("cap.demo", "demo", true)]).unwrap();
        assert!(validate_strict_registry(&demo, 10).is_err());

        let duplicate =
            serde_json::to_string(&vec![record("cap.a", "provider-a", true); 2]).unwrap();
        assert!(validate_strict_registry(&duplicate, 10).is_err());
    }

    #[test]
    fn strict_registry_requires_an_enabled_production_record() {
        let disabled =
            serde_json::to_string(&vec![record("cap.a", "provider-a", false)]).unwrap();
        assert!(validate_strict_registry(&disabled, 10).is_err());

        let valid = serde_json::to_string(&vec![record("cap.a", "provider-a", true)]).unwrap();
        let records = validate_strict_registry(&valid, 10).unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].enabled);
    }
}
