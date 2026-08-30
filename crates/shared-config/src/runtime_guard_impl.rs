use sqlx::postgres::PgPoolOptions;
use std::{env, error::Error, fmt, time::Duration};
use uuid::Uuid;

pub const CONFIG_ERROR_EXIT_CODE: i32 = 78;
const DEFAULT_DATABASE_TIMEOUT_SECONDS: u64 = 5;
const MAX_DATABASE_TIMEOUT_SECONDS: u64 = 60;

const IDENTITY_ADMIN_SOURCES: &[&str] = &["IDENTITY_ADMIN_TOKENS_JSON", "IDENTITY_ADMIN_TOKEN"];
const AUDIT_ADMIN_SOURCES: &[&str] = &[
    "AUDIT_ADMIN_TOKENS_JSON",
    "AUDIT_ADMIN_TOKEN",
    "IDENTITY_ADMIN_TOKENS_JSON",
    "IDENTITY_ADMIN_TOKEN",
];
const EXECUTION_ADMIN_SOURCES: &[&str] = &[
    "EXECUTION_ADMIN_TOKENS_JSON",
    "EXECUTION_ADMIN_TOKEN",
    "IDENTITY_ADMIN_TOKENS_JSON",
    "IDENTITY_ADMIN_TOKEN",
];
const LEDGER_ADMIN_SOURCES: &[&str] = &[
    "LEDGER_ADMIN_TOKENS_JSON",
    "LEDGER_ADMIN_TOKEN",
    "IDENTITY_ADMIN_TOKENS_JSON",
    "IDENTITY_ADMIN_TOKEN",
];

const SENSITIVE_ENV_NAMES: &[&str] = &[
    "DATABASE_URL",
    "IDENTITY_STATIC_API_KEYS_JSON",
    "IDENTITY_ADMIN_TOKENS_JSON",
    "IDENTITY_ADMIN_TOKEN",
    "AUDIT_ADMIN_TOKENS_JSON",
    "AUDIT_ADMIN_TOKEN",
    "EXECUTION_ADMIN_TOKENS_JSON",
    "EXECUTION_ADMIN_TOKEN",
    "EXECUTION_WORKER_ADMIN_TOKEN",
    "LEDGER_ADMIN_TOKENS_JSON",
    "LEDGER_ADMIN_TOKEN",
    "CEX_GATEWAY_API_KEY",
    "CEX_INTERNAL_SERVICE_TOKEN",
    "CONSUMER_ENTRY_INGRESS_TOKEN",
    "MATRIX_ENTRY_INGRESS_TOKEN",
    "CONSUMER_ENTRY_SESSION_AUTH_SECRET",
    "MATRIX_ENTRY_CONSUMER_SESSION_AUTH_SECRET",
    "TRNM_VALUE_ENTITLEMENT_SIGNING_SECRET",
    "TRNM_GAME_AUTHORITY_TOKEN",
    "TRNM_PLAYER_SESSION_SIGNING_SECRET",
];

const WEAK_MARKERS: &[&str] = &[
    "local-dev-key",
    "local-dev-admin-token",
    "local-development-",
    "change-me",
    "changeme",
    "replace-me",
    "replace_",
    "insecure-default",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceKind {
    Gateway,
    Identity,
    Ledger,
    Execution,
    Audit,
}

/// Parse a boolean environment value using the same normalization everywhere in
/// the service startup path.  Keeping this helper public prevents a binary's
/// post-guard configuration code from accidentally treating a value accepted by
/// the production guard (for example `ON` or ` true `) as false.
pub fn parse_bool_value(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Resolve a possibly-present boolean value with the shared normalization
/// rules.  Malformed input falls back to the caller-supplied fail-closed
/// default; production-like startup validation rejects malformed values before
/// service state is constructed.
fn env_flag_value(raw: Option<&str>, default: bool) -> bool {
    raw.and_then(parse_bool_value).unwrap_or(default)
}

/// Read a boolean environment flag with the shared normalization rules.
///
/// The supplied default is part of the security contract.  In particular,
/// safety controls whose production default is `true` must not be disabled by
/// a misspelled value such as `tru` or `enabled`.  Production-like profiles
/// additionally reject malformed values and explicitly-disabled required
/// controls in `validate_production_boolean_configuration`.
pub fn env_flag(name: &str, default: bool) -> bool {
    let raw = env::var(name).ok();
    env_flag_value(raw.as_deref(), default)
}

impl ServiceKind {
    fn fail_fast_env(self) -> &'static str {
        match self {
            Self::Gateway => "GATEWAY_FAIL_FAST",
            Self::Identity => "IDENTITY_FAIL_FAST",
            Self::Ledger => "LEDGER_FAIL_FAST",
            Self::Execution => "EXECUTION_FAIL_FAST",
            Self::Audit => "AUDIT_FAIL_FAST",
        }
    }
}

impl fmt::Display for ServiceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Gateway => "gateway-service",
            Self::Identity => "identity-service",
            Self::Ledger => "ledger-service",
            Self::Execution => "execution-service",
            Self::Audit => "audit-service",
        };
        formatter.write_str(value)
    }
}

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
    pub fn parse(raw: &str) -> Result<Self, StartupError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" => Ok(Self::Test),
            "local" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "staging" | "stage" => Ok(Self::Staging),
            // The native TRNM economy launcher uses a lane-specific profile
            // name so the runtime manager can select the ledger + consumer
            // topology.  It still has to receive the same production-like
            // startup posture as the canonical production profile.
            "production" | "prod" | "trnm-economy" | "trnm_economy" => Ok(Self::Production),
            other => Err(StartupError::new(
                "invalid_runtime_profile",
                format!(
                    "unsupported runtime profile '{other}'; expected test, local, dev, beta, staging, production, or trnm-economy"
                ),
            )),
        }
    }

    pub fn is_production_like(self) -> bool {
        matches!(self, Self::Beta | Self::Staging | Self::Production)
    }
}

impl fmt::Display for RuntimeProfile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Test => "test",
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Staging => "staging",
            Self::Production => "production",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct StartupReport {
    pub service: ServiceKind,
    pub profile: RuntimeProfile,
    pub database_preflight: bool,
    pub identity_static_fallback_disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupError {
    pub code: &'static str,
    pub message: String,
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

impl Error for StartupError {}

pub async fn enforce(service: ServiceKind) -> Result<StartupReport, StartupError> {
    let profile = resolve_runtime_profile()?;
    let mut database_preflight = false;
    let mut identity_static_fallback_disabled = false;

    if profile.is_production_like() {
        validate_production_posture(service)?;

        if matches!(service, ServiceKind::Identity) {
            install_identity_fail_closed_static_sink()?;
            identity_static_fallback_disabled = true;
        }

        preflight_database().await?;
        database_preflight = true;
    }

    Ok(StartupReport {
        service,
        profile,
        database_preflight,
        identity_static_fallback_disabled,
    })
}

pub fn resolve_runtime_profile() -> Result<RuntimeProfile, StartupError> {
    let cex_profile = get_trimmed_env("CEX_RUNTIME_PROFILE");
    let app_env = get_trimmed_env("APP_ENV");
    let allow_implicit_dev = match get_trimmed_env("CEX_ALLOW_IMPLICIT_DEV_PROFILE") {
        Some(raw) => parse_bool("CEX_ALLOW_IMPLICIT_DEV_PROFILE", &raw)?,
        None => false,
    };

    resolve_profile_values(
        cex_profile.as_deref(),
        app_env.as_deref(),
        allow_implicit_dev,
    )
}

fn resolve_profile_values(
    cex_profile: Option<&str>,
    app_env: Option<&str>,
    allow_implicit_dev: bool,
) -> Result<RuntimeProfile, StartupError> {
    let cex_profile = cex_profile.map(RuntimeProfile::parse).transpose()?;
    let app_env = app_env.map(RuntimeProfile::parse).transpose()?;

    match (cex_profile, app_env) {
        (Some(primary), Some(compatibility)) if primary != compatibility => Err(StartupError::new(
            "runtime_profile_conflict",
            format!(
                "CEX_RUNTIME_PROFILE resolves to {primary}, but APP_ENV resolves to {compatibility}"
            ),
        )),
        (Some(profile), _) | (_, Some(profile)) => Ok(profile),
        (None, None) if allow_implicit_dev => Ok(RuntimeProfile::Dev),
        (None, None) => Err(StartupError::new(
            "runtime_profile_missing",
            "set CEX_RUNTIME_PROFILE or APP_ENV explicitly; implicit dev is disabled",
        )),
    }
}

fn validate_production_posture(service: ServiceKind) -> Result<(), StartupError> {
    require_non_empty("DATABASE_URL")?;
    require_explicit_true(service.fail_fast_env())?;
    validate_production_boolean_configuration(service)?;
    validate_no_weak_credentials()?;
    validate_admin_configuration(service)?;

    if matches!(service, ServiceKind::Identity)
        && get_trimmed_env("IDENTITY_STATIC_API_KEYS_JSON").is_some()
    {
        return Err(StartupError::new(
            "identity_static_keys_forbidden",
            "IDENTITY_STATIC_API_KEYS_JSON is forbidden in production-like profiles",
        ));
    }

    Ok(())
}

fn validate_production_boolean_configuration(
    service: ServiceKind,
) -> Result<(), StartupError> {
    if !matches!(service, ServiceKind::Ledger) {
        return Ok(());
    }

    // These controls default to LEDGER_FAIL_FAST in AppState and therefore
    // resolve to true in every production-like ledger.  An explicit false
    // would weaken exact trace or player/session ownership at the point where
    // startup is expected to be fail-closed, so reject it rather than silently
    // changing the authority model.
    for name in [
        "LEDGER_V2_REQUIRE_EXPLICIT_TRACE",
        "TRNM_REQUIRE_PLAYER_SESSION",
    ] {
        require_true_or_unset(name, get_trimmed_env(name).as_deref())?;
    }

    // System operations have an explicit production use in the dedicated TRNM
    // launcher, while the general production default remains false.  Both
    // boolean values are valid, but malformed input must never be coerced into
    // another authority posture.
    validate_optional_boolean(
        "TRNM_ALLOW_SYSTEM_ECONOMY_OPERATIONS",
        get_trimmed_env("TRNM_ALLOW_SYSTEM_ECONOMY_OPERATIONS").as_deref(),
    )?;

    Ok(())
}

fn require_true_or_unset(name: &str, raw: Option<&str>) -> Result<(), StartupError> {
    let Some(raw) = raw else {
        return Ok(());
    };
    if parse_bool(name, raw)? {
        Ok(())
    } else {
        Err(StartupError::new(
            "production_control_disabled",
            format!("{name} must not be false in production-like ledger profiles"),
        ))
    }
}

fn validate_optional_boolean(name: &str, raw: Option<&str>) -> Result<(), StartupError> {
    if let Some(raw) = raw {
        parse_bool(name, raw)?;
    }
    Ok(())
}

fn validate_admin_configuration(service: ServiceKind) -> Result<(), StartupError> {
    match service {
        ServiceKind::Gateway => {
            require_any_non_empty("gateway ledger admin credential", LEDGER_ADMIN_SOURCES)?;
            require_any_non_empty(
                "gateway execution admin credential",
                EXECUTION_ADMIN_SOURCES,
            )?;
        }
        ServiceKind::Identity => {
            require_any_non_empty("identity admin credential", IDENTITY_ADMIN_SOURCES)?;
        }
        ServiceKind::Ledger => {
            require_any_non_empty("ledger admin credential", LEDGER_ADMIN_SOURCES)?;
        }
        ServiceKind::Execution => {
            require_any_non_empty("execution admin credential", EXECUTION_ADMIN_SOURCES)?;
        }
        ServiceKind::Audit => {
            require_any_non_empty("audit admin credential", AUDIT_ADMIN_SOURCES)?;
        }
    }

    Ok(())
}

fn require_any_non_empty(label: &str, names: &[&str]) -> Result<(), StartupError> {
    if names.iter().any(|name| get_trimmed_env(name).is_some()) {
        return Ok(());
    }

    Err(StartupError::new(
        "required_credential_missing",
        format!("{label} is missing; configure one of {}", names.join(", ")),
    ))
}

fn require_non_empty(name: &str) -> Result<String, StartupError> {
    get_trimmed_env(name).ok_or_else(|| {
        StartupError::new(
            "required_configuration_missing",
            format!("{name} must be configured and non-empty"),
        )
    })
}

fn require_explicit_true(name: &str) -> Result<(), StartupError> {
    let raw = get_trimmed_env(name).ok_or_else(|| {
        StartupError::new(
            "fail_fast_not_configured",
            format!("{name}=true is required in production-like profiles"),
        )
    })?;

    if parse_bool(name, &raw)? {
        Ok(())
    } else {
        Err(StartupError::new(
            "fail_fast_disabled",
            format!("{name} must be true in production-like profiles"),
        ))
    }
}

fn parse_bool(name: &str, raw: &str) -> Result<bool, StartupError> {
    parse_bool_value(raw).ok_or_else(|| {
        StartupError::new(
            "invalid_boolean_configuration",
            format!("{name} must be one of true/false, 1/0, yes/no, or on/off"),
        )
    })
}

fn validate_no_weak_credentials() -> Result<(), StartupError> {
    for name in SENSITIVE_ENV_NAMES {
        let Some(value) = get_trimmed_env(name) else {
            continue;
        };
        if let Some(marker) = weak_marker(&value) {
            return Err(StartupError::new(
                "unsafe_default_rejected",
                format!("{name} contains forbidden development/placeholder marker '{marker}'"),
            ));
        }
    }
    Ok(())
}

fn weak_marker(value: &str) -> Option<&'static str> {
    let lowered = value.to_ascii_lowercase();
    WEAK_MARKERS
        .iter()
        .copied()
        .find(|marker| lowered.contains(marker))
}

fn install_identity_fail_closed_static_sink() -> Result<(), StartupError> {
    if get_trimmed_env("IDENTITY_STATIC_API_KEYS_JSON").is_some() {
        return Err(StartupError::new(
            "identity_static_keys_forbidden",
            "cannot install the deny-only sink while IDENTITY_STATIC_API_KEYS_JSON is configured",
        ));
    }

    let sink_key = format!(
        "cex_disabled_static_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let sink = build_identity_static_sink(&sink_key);

    // The process has not started worker threads yet. The value is random and is never logged.
    unsafe {
        env::set_var("IDENTITY_STATIC_API_KEYS_JSON", sink);
    }
    Ok(())
}

fn build_identity_static_sink(sink_key: &str) -> String {
    serde_json::json!([{
        "api_key": sink_key,
        "org_id": Uuid::nil().to_string(),
        "actor_id": "disabled-static-fallback",
        "actor_label": "Disabled Static Fallback"
    }])
    .to_string()
}

async fn preflight_database() -> Result<(), StartupError> {
    let database_url = require_non_empty("DATABASE_URL")?;
    let timeout_seconds = startup_database_timeout_seconds()?;
    let timeout_duration = Duration::from_secs(timeout_seconds);

    let pool = tokio::time::timeout(
        timeout_duration,
        PgPoolOptions::new()
            .max_connections(1)
            .connect(&database_url),
    )
    .await
    .map_err(|_| {
        StartupError::new(
            "database_preflight_timeout",
            format!("PostgreSQL connection preflight timed out after {timeout_seconds}s"),
        )
    })?
    .map_err(|error| {
        StartupError::new(
            "database_preflight_failed",
            format!("PostgreSQL connection preflight failed: {error}"),
        )
    })?;

    let probe = tokio::time::timeout(
        timeout_duration,
        sqlx::query_scalar::<_, i32>("select 1").fetch_one(&pool),
    )
    .await;
    pool.close().await;

    let value = probe
        .map_err(|_| {
            StartupError::new(
                "database_probe_timeout",
                format!("PostgreSQL readiness query timed out after {timeout_seconds}s"),
            )
        })?
        .map_err(|error| {
            StartupError::new(
                "database_probe_failed",
                format!("PostgreSQL readiness query failed: {error}"),
            )
        })?;

    if value != 1 {
        return Err(StartupError::new(
            "database_probe_invalid",
            "PostgreSQL readiness query returned an unexpected value",
        ));
    }

    Ok(())
}

fn startup_database_timeout_seconds() -> Result<u64, StartupError> {
    parse_timeout_seconds(get_trimmed_env("CEX_STARTUP_DATABASE_TIMEOUT_SECONDS").as_deref())
}

fn parse_timeout_seconds(raw: Option<&str>) -> Result<u64, StartupError> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_DATABASE_TIMEOUT_SECONDS);
    };

    let value = raw.parse::<u64>().map_err(|_| {
        StartupError::new(
            "invalid_database_timeout",
            "CEX_STARTUP_DATABASE_TIMEOUT_SECONDS must be an integer",
        )
    })?;

    if !(1..=MAX_DATABASE_TIMEOUT_SECONDS).contains(&value) {
        return Err(StartupError::new(
            "invalid_database_timeout",
            format!(
                "CEX_STARTUP_DATABASE_TIMEOUT_SECONDS must be between 1 and {MAX_DATABASE_TIMEOUT_SECONDS}"
            ),
        ));
    }

    Ok(value)
}

fn get_trimmed_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_aliases_are_normalized() {
        assert_eq!(
            RuntimeProfile::parse("development").unwrap(),
            RuntimeProfile::Dev
        );
        assert_eq!(
            RuntimeProfile::parse("prod").unwrap(),
            RuntimeProfile::Production
        );
        assert_eq!(
            RuntimeProfile::parse("stage").unwrap(),
            RuntimeProfile::Staging
        );
        assert_eq!(
            RuntimeProfile::parse("trnm-economy").unwrap(),
            RuntimeProfile::Production
        );
    }

    #[test]
    fn production_like_profiles_are_explicit() {
        assert!(!RuntimeProfile::Dev.is_production_like());
        assert!(!RuntimeProfile::Local.is_production_like());
        assert!(RuntimeProfile::Beta.is_production_like());
        assert!(RuntimeProfile::Staging.is_production_like());
        assert!(RuntimeProfile::Production.is_production_like());
    }

    #[test]
    fn matching_profile_sources_are_accepted() {
        let profile = resolve_profile_values(Some("prod"), Some("production"), false).unwrap();
        assert_eq!(profile, RuntimeProfile::Production);
    }

    #[test]
    fn conflicting_profile_sources_are_rejected() {
        let error = resolve_profile_values(Some("production"), Some("dev"), false).unwrap_err();
        assert_eq!(error.code, "runtime_profile_conflict");
    }

    #[test]
    fn missing_profile_requires_explicit_escape_hatch() {
        let error = resolve_profile_values(None, None, false).unwrap_err();
        assert_eq!(error.code, "runtime_profile_missing");
        assert_eq!(
            resolve_profile_values(None, None, true).unwrap(),
            RuntimeProfile::Dev
        );
    }

    #[test]
    fn weak_markers_detect_known_defaults_and_placeholders() {
        assert_eq!(
            weak_marker("local-dev-admin-token"),
            Some("local-dev-admin-token")
        );
        assert_eq!(
            weak_marker("postgres://cex:REPLACE_ME@db/cex"),
            Some("replace_")
        );
        assert_eq!(weak_marker("high-entropy-production-token"), None);
    }

    #[test]
    fn timeout_bounds_are_enforced() {
        assert_eq!(parse_timeout_seconds(None).unwrap(), 5);
        assert_eq!(parse_timeout_seconds(Some("1")).unwrap(), 1);
        assert_eq!(parse_timeout_seconds(Some("60")).unwrap(), 60);
        assert!(parse_timeout_seconds(Some("0")).is_err());
        assert!(parse_timeout_seconds(Some("61")).is_err());
        assert!(parse_timeout_seconds(Some("five")).is_err());
    }

    #[test]
    fn boolean_values_share_trimmed_case_insensitive_normalization() {
        for raw in ["1", "true", "TRUE", "TrUe", " yes ", "YES", "on", "ON"] {
            assert_eq!(parse_bool_value(raw), Some(true), "raw={raw:?}");
        }
        for raw in ["0", "false", "FALSE", "FaLsE", " no ", "NO", "off", "OFF"] {
            assert_eq!(parse_bool_value(raw), Some(false), "raw={raw:?}");
        }
    }

    #[test]
    fn malformed_boolean_values_are_not_coerced_to_true() {
        for raw in ["", "maybe", "truthy", "2", "true-ish"] {
            assert_eq!(parse_bool_value(raw), None, "raw={raw:?}");
        }
    }

    #[test]
    fn malformed_environment_flags_keep_the_fail_closed_default() {
        assert!(env_flag_value(Some("not-a-boolean"), true));
        assert!(!env_flag_value(Some("not-a-boolean"), false));
        assert!(env_flag_value(None, true));
        assert!(!env_flag_value(None, false));
    }

    #[test]
    fn production_ledger_controls_reject_false_and_malformed_values() {
        assert!(require_true_or_unset("CONTROL", None).is_ok());
        assert!(require_true_or_unset("CONTROL", Some("ON")).is_ok());

        let disabled = require_true_or_unset("CONTROL", Some("false")).unwrap_err();
        assert_eq!(disabled.code, "production_control_disabled");

        let malformed = require_true_or_unset("CONTROL", Some("enabled")).unwrap_err();
        assert_eq!(malformed.code, "invalid_boolean_configuration");

        assert!(validate_optional_boolean("OPTIONAL", None).is_ok());
        assert!(validate_optional_boolean("OPTIONAL", Some("false")).is_ok());
        assert_eq!(
            validate_optional_boolean("OPTIONAL", Some("sometimes"))
                .unwrap_err()
                .code,
            "invalid_boolean_configuration"
        );
    }

    #[test]
    fn identity_sink_has_exact_non_authoritative_shape() {
        let raw = build_identity_static_sink("unpublished-random-sink");
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let item = value.as_array().unwrap().first().unwrap();
        assert_eq!(item["api_key"], "unpublished-random-sink");
        assert_eq!(item["org_id"], Uuid::nil().to_string());
        assert_eq!(item["actor_id"], "disabled-static-fallback");
    }
}
