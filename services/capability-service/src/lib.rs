use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use shared_types::CapabilityRecord;
use std::{collections::HashMap, env, sync::Arc};

const REGISTRY_ENV: &str = "CAPABILITY_EXTERNAL_AGENT_REGISTRY_JSON";
const MAX_REGISTRY_BYTES: usize = 1_048_576;
const MAX_REGISTRY_RECORDS: usize = 1_024;
const MAX_ID_BYTES: usize = 192;
const MAX_REFERENCE_BYTES: usize = 512;
const MAX_DISPLAY_BYTES: usize = 256;
const MAX_VERSION_BYTES: usize = 128;
const MAX_DESCRIPTION_BYTES: usize = 4_096;
const REQUIRED_KEYS: &[&str] = &[
    "capability_id",
    "kind",
    "provider",
    "provider_ref",
    "display_name",
    "version",
    "enabled",
];
const ALLOWED_KEYS: &[&str] = &[
    "capability_id",
    "kind",
    "provider",
    "provider_ref",
    "display_name",
    "version",
    "description",
    "enabled",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeProfile {
    Test,
    Local,
    Dev,
    Beta,
    Staging,
    Production,
}

impl RuntimeProfile {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" => Ok(Self::Test),
            "local" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "staging" | "stage" => Ok(Self::Staging),
            "production" | "prod" | "trnm-economy" | "trnm_economy" => {
                Ok(Self::Production)
            }
            other => Err(format!(
                "unsupported runtime profile '{other}'; expected test, local, dev, beta, staging, production, or trnm-economy"
            )),
        }
    }

    fn is_production_like(self) -> bool {
        matches!(self, Self::Beta | Self::Staging | Self::Production)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Local => "local",
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    capabilities: Arc<HashMap<String, CapabilityRecord>>,
    ready: bool,
    source: Arc<str>,
}

impl AppState {
    pub async fn from_env() -> Result<Self, String> {
        let profile = resolve_runtime_profile()?;
        let registry = trimmed_env(REGISTRY_ENV);

        match registry {
            Some(raw) => Self::from_registry_json(&raw, "environment"),
            None if profile.is_production_like() => Err(format!(
                "{REGISTRY_ENV} is required in production-like profiles; local model discovery and implicit demo capabilities are forbidden"
            )),
            None => Ok(Self {
                capabilities: Arc::new(HashMap::new()),
                ready: false,
                source: Arc::from("empty-development-registry"),
            }),
        }
    }

    pub fn from_registry_json_for_tests(raw: &str) -> Result<Self, String> {
        Self::from_registry_json(raw, "test")
    }

    pub fn new_for_tests(records: Vec<CapabilityRecord>) -> Self {
        Self::from_records(records, "test", true).expect("test capability records must be valid")
    }

    pub fn new_empty_for_tests() -> Self {
        Self {
            capabilities: Arc::new(HashMap::new()),
            ready: false,
            source: Arc::from("empty-test-registry"),
        }
    }

    fn from_registry_json(raw: &str, source: &str) -> Result<Self, String> {
        if raw.len() > MAX_REGISTRY_BYTES {
            return Err(format!(
                "{REGISTRY_ENV} exceeds the {MAX_REGISTRY_BYTES}-byte limit"
            ));
        }

        let value: Value =
            serde_json::from_str(raw).map_err(|error| format!("decode {REGISTRY_ENV}: {error}"))?;
        let entries = value
            .as_array()
            .ok_or_else(|| format!("{REGISTRY_ENV} must be a JSON array"))?;
        if entries.is_empty() {
            return Err(format!("{REGISTRY_ENV} must contain at least one record"));
        }
        if entries.len() > MAX_REGISTRY_RECORDS {
            return Err(format!(
                "{REGISTRY_ENV} exceeds the {MAX_REGISTRY_RECORDS}-record limit"
            ));
        }

        let mut records = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            validate_entry_shape(index, entry)?;
            records.push(
                serde_json::from_value::<CapabilityRecord>(entry.clone()).map_err(|error| {
                    format!("decode {REGISTRY_ENV}[{index}] as a capability record: {error}")
                })?,
            );
        }
        Self::from_records(records, source, true)
    }

    fn from_records(
        records: Vec<CapabilityRecord>,
        source: &str,
        ready: bool,
    ) -> Result<Self, String> {
        if records.len() > MAX_REGISTRY_RECORDS {
            return Err(format!(
                "capability registry exceeds the {MAX_REGISTRY_RECORDS}-record limit"
            ));
        }

        let mut map = HashMap::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            validate_record(index, &record)?;
            if map.insert(record.capability_id.clone(), record).is_some() {
                return Err(format!(
                    "capability registry contains a duplicate capability_id at index {index}"
                ));
            }
        }

        Ok(Self {
            capabilities: Arc::new(map),
            ready,
            source: Arc::from(source),
        })
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/v1/capabilities", get(list_capabilities))
        .route("/v1/capabilities/:id", get(get_capability))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Response {
    let status = if state.ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(json!({
            "service": "capability-service",
            "ready": state.ready,
            "runtime_policy": "external_only",
            "registry_source": state.source.as_ref(),
            "record_count": state.capabilities.len(),
            "production_authorization": "not_granted"
        })),
    )
        .into_response()
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let total = state.capabilities.len();
    let enabled = state
        .capabilities
        .values()
        .filter(|record| record.enabled)
        .count();
    let disabled = total.saturating_sub(enabled);
    let ready = if state.ready { 1 } else { 0 };
    let body = format!(
        concat!(
            "# HELP cex_capability_service_up Whether capability-service metrics are being served.\n",
            "# TYPE cex_capability_service_up gauge\n",
            "cex_capability_service_up 1\n",
            "# HELP cex_capability_service_ready Whether a validated external-Agent registry is active.\n",
            "# TYPE cex_capability_service_ready gauge\n",
            "cex_capability_service_ready {ready}\n",
            "# HELP cex_capability_records_total External-Agent capability declarations grouped by enabled state.\n",
            "# TYPE cex_capability_records_total gauge\n",
            "cex_capability_records_total{{state=\"total\"}} {total}\n",
            "cex_capability_records_total{{state=\"enabled\"}} {enabled}\n",
            "cex_capability_records_total{{state=\"disabled\"}} {disabled}\n",
        ),
        ready = ready,
        total = total,
        enabled = enabled,
        disabled = disabled,
    );
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}

async fn list_capabilities(State(state): State<AppState>) -> Json<Vec<CapabilityRecord>> {
    let mut records = state.capabilities.values().cloned().collect::<Vec<_>>();
    records.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));
    Json(records)
}

async fn get_capability(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    match state.capabilities.get(&id) {
        Some(record) => (StatusCode::OK, Json(record.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "capability not found" })),
        )
            .into_response(),
    }
}

fn resolve_runtime_profile() -> Result<RuntimeProfile, String> {
    let cex_profile = trimmed_env("CEX_RUNTIME_PROFILE");
    let app_env = trimmed_env("APP_ENV");
    let allow_implicit_dev = match trimmed_env("CEX_ALLOW_IMPLICIT_DEV_PROFILE") {
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
) -> Result<RuntimeProfile, String> {
    let primary = cex_profile.map(RuntimeProfile::parse).transpose()?;
    let compatibility = app_env.map(RuntimeProfile::parse).transpose()?;
    match (primary, compatibility) {
        (Some(left), Some(right)) if left != right => Err(format!(
            "CEX_RUNTIME_PROFILE resolves to {}, but APP_ENV resolves to {}",
            left.name(),
            right.name()
        )),
        (Some(profile), _) | (_, Some(profile)) => Ok(profile),
        (None, None) if allow_implicit_dev => Ok(RuntimeProfile::Dev),
        (None, None) => Err(
            "set CEX_RUNTIME_PROFILE or APP_ENV explicitly; implicit dev is disabled".to_string(),
        ),
    }
}

fn parse_bool(name: &str, raw: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!(
            "{name} must be one of true/false, 1/0, yes/no, or on/off"
        )),
    }
}

fn trimmed_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_entry_shape(index: usize, entry: &Value) -> Result<(), String> {
    let object = entry
        .as_object()
        .ok_or_else(|| format!("{REGISTRY_ENV}[{index}] must be a JSON object"))?;

    for key in object.keys() {
        if !ALLOWED_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "{REGISTRY_ENV}[{index}] contains unsupported field '{key}'"
            ));
        }
    }
    for key in REQUIRED_KEYS {
        if !object.contains_key(*key) {
            return Err(format!(
                "{REGISTRY_ENV}[{index}] is missing required field '{key}'"
            ));
        }
    }
    Ok(())
}

fn validate_record(index: usize, record: &CapabilityRecord) -> Result<(), String> {
    validate_text(index, "capability_id", &record.capability_id, MAX_ID_BYTES)?;
    validate_text(index, "kind", &record.kind, MAX_ID_BYTES)?;
    validate_text(index, "provider", &record.provider, MAX_ID_BYTES)?;
    validate_text(
        index,
        "provider_ref",
        &record.provider_ref,
        MAX_REFERENCE_BYTES,
    )?;
    validate_text(
        index,
        "display_name",
        &record.display_name,
        MAX_DISPLAY_BYTES,
    )?;
    validate_text(index, "version", &record.version, MAX_VERSION_BYTES)?;
    if let Some(description) = record.description.as_deref() {
        validate_text(index, "description", description, MAX_DESCRIPTION_BYTES)?;
    }

    if !record.capability_id.starts_with("cap.external-agent.") {
        return Err(format!(
            "capability registry record {index} capability_id must start with 'cap.external-agent.'"
        ));
    }
    if record.kind != "external_agent_capability" {
        return Err(format!(
            "capability registry record {index} kind must equal 'external_agent_capability'"
        ));
    }
    if record.provider != "external-agent" {
        return Err(format!(
            "capability registry record {index} provider must equal 'external-agent'"
        ));
    }
    if !(record.provider_ref.starts_with("did:") || record.provider_ref.starts_with("agent:")) {
        return Err(format!(
            "capability registry record {index} provider_ref must be a stable did: or agent: reference"
        ));
    }
    Ok(())
}

fn validate_text(index: usize, field: &str, value: &str, max_bytes: usize) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "capability registry record {index} field '{field}' must be non-empty"
        ));
    }
    if value.len() > max_bytes {
        return Err(format!(
            "capability registry record {index} field '{field}' exceeds {max_bytes} bytes"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "capability registry record {index} field '{field}' contains a control character"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_profile_values, AppState, RuntimeProfile};

    #[test]
    fn strict_registry_rejects_local_model_authority() {
        let error = AppState::from_registry_json_for_tests(
            r#"[{"capability_id":"cap.openclaw.model.demo","kind":"model","provider":"openclaw","provider_ref":"openclaw/demo","display_name":"demo","version":"v1","enabled":true}]"#,
        )
        .err()
        .expect("local model authority must be rejected");
        assert!(error.contains("cap.external-agent."));
    }

    #[test]
    fn strict_registry_rejects_unknown_fields() {
        let error = AppState::from_registry_json_for_tests(
            r#"[{"capability_id":"cap.external-agent.demo","kind":"external_agent_capability","provider":"external-agent","provider_ref":"did:trnm:demo#cap","display_name":"demo","version":"v1","enabled":true,"secret":"forbidden"}]"#,
        )
        .err()
        .expect("unknown fields must fail closed");
        assert!(error.contains("unsupported field 'secret'"));
    }

    #[test]
    fn production_aliases_are_production_like() {
        for raw in [
            "beta",
            "stage",
            "staging",
            "prod",
            "production",
            "trnm-economy",
        ] {
            let profile = resolve_profile_values(Some(raw), None, false)
                .expect("production-like profile must parse");
            assert!(profile.is_production_like());
        }
    }

    #[test]
    fn explicit_profile_conflicts_and_missing_profile_fail_closed() {
        assert!(resolve_profile_values(Some("dev"), Some("production"), false).is_err());
        assert!(resolve_profile_values(None, None, false).is_err());
        assert_eq!(
            resolve_profile_values(None, None, true).expect("explicit implicit-dev opt-in"),
            RuntimeProfile::Dev
        );
    }
}
