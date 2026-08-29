use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use shared_types::CapabilityRecord;
use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    sync::Arc,
};
use tokio::process::Command;

#[derive(Clone)]
pub struct AppState {
    capabilities: Arc<HashMap<String, CapabilityRecord>>,
}

impl AppState {
    pub async fn from_env() -> Self {
        Self {
            capabilities: Arc::new(load_capabilities().await),
        }
    }

    pub fn new_for_tests(records: Vec<CapabilityRecord>) -> Self {
        let map = records
            .into_iter()
            .map(|record| (record.capability_id.clone(), record))
            .collect();
        Self {
            capabilities: Arc::new(map),
        }
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

async fn health() -> &'static str {
    "capability-service ok"
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let total = state.capabilities.len();
    let enabled = state
        .capabilities
        .values()
        .filter(|record| record.enabled)
        .count();
    let disabled = total.saturating_sub(enabled);
    let body = format!(
        concat!(
            "# HELP cex_capability_service_up Whether capability-service metrics are being served.\n",
            "# TYPE cex_capability_service_up gauge\n",
            "cex_capability_service_up 1\n",
            "# HELP cex_capability_records_total Capability registry records grouped by enabled state.\n",
            "# TYPE cex_capability_records_total gauge\n",
            "cex_capability_records_total{{state=\"total\"}} {total}\n",
            "cex_capability_records_total{{state=\"enabled\"}} {enabled}\n",
            "cex_capability_records_total{{state=\"disabled\"}} {disabled}\n",
        ),
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

async fn get_capability(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> axum::response::Response {
    match state.capabilities.get(&id) {
        Some(record) => (StatusCode::OK, Json(record.clone())).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "capability not found" })),
        )
            .into_response(),
    }
}

async fn load_capabilities() -> HashMap<String, CapabilityRecord> {
    let mut records = env::var("CAPABILITY_STATIC_REGISTRY_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<CapabilityRecord>>(&raw).ok())
        .filter(|records| !records.is_empty())
        .unwrap_or_else(default_capabilities);

    records.extend(load_openclaw_capabilities_from_env().await);

    records
        .into_iter()
        .map(|record| (record.capability_id.clone(), record))
        .collect()
}

async fn load_openclaw_capabilities_from_env() -> Vec<CapabilityRecord> {
    let Some(path) = resolve_openclaw_models_path() else {
        return Vec::new();
    };

    let Ok(raw) = tokio::fs::read_to_string(&path).await else {
        return Vec::new();
    };

    let Ok(catalog) = serde_json::from_str::<OpenClawModelsCatalog>(&raw) else {
        return Vec::new();
    };

    let allowed_model_keys = load_openclaw_allowed_model_keys().await.ok();
    build_openclaw_capabilities(&catalog, allowed_model_keys.as_ref())
}

fn resolve_openclaw_models_path() -> Option<PathBuf> {
    let raw = env::var("CAPABILITY_OPENCLAW_MODELS_JSON_PATH")
        .ok()
        .or_else(|| env::var("OPENCLAW_MODELS_JSON_PATH").ok())
        .or_else(|| {
            env::var("OPENCLAW_AGENT_DIR")
                .ok()
                .map(|agent_dir| format!("{}/models.json", agent_dir.trim_end_matches('/')))
        })
        .unwrap_or_else(|| "~/.openclaw/agents/main/agent/models.json".to_string());

    let path = expand_tilde(&raw);
    path.exists().then_some(path)
}

async fn load_openclaw_allowed_model_keys() -> Result<HashSet<String>, String> {
    let cli_bin = env::var("OPENCLAW_CLI_BIN").unwrap_or_else(|_| "openclaw".to_string());
    let mut command = Command::new(&cli_bin);
    apply_openclaw_scope_env(&mut command);
    let output = command
        .args(["models", "status", "--json"])
        .output()
        .await
        .map_err(|err| format!("spawn openclaw models status failed: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!(
            "openclaw models status returned {}: {detail}",
            output.status
        ));
    }

    let status: OpenClawModelsStatus = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("decode openclaw models status failed: {err}"))?;

    Ok(status.allowed.into_iter().collect())
}

fn build_openclaw_capabilities(
    catalog: &OpenClawModelsCatalog,
    allowed_model_keys: Option<&HashSet<String>>,
) -> Vec<CapabilityRecord> {
    let mut seen_model_keys = HashSet::new();
    let mut records = Vec::new();

    for (source_provider, provider) in &catalog.providers {
        for model in &provider.models {
            let provider_ref = model.id.trim();
            if provider_ref.is_empty() {
                continue;
            }

            let canonical_provider =
                canonical_openclaw_provider(source_provider, provider.api.as_deref());
            let model_key = format!("{canonical_provider}/{provider_ref}");
            if !seen_model_keys.insert(model_key.clone()) {
                continue;
            }

            let kind = infer_capability_kind(model);
            let display_name = model
                .name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| provider_ref.to_string());
            let enabled = allowed_model_keys
                .map(|allowed| allowed.contains(&model_key))
                .unwrap_or(true);

            records.push(CapabilityRecord {
                capability_id: format!(
                    "cap.openclaw.{}.{}.{}",
                    kind,
                    sanitize_capability_segment(canonical_provider),
                    sanitize_capability_segment(provider_ref)
                ),
                kind: kind.to_string(),
                provider: canonical_provider.to_string(),
                provider_ref: provider_ref.to_string(),
                display_name: format!("OpenClaw {display_name}"),
                version: "openclaw-local".to_string(),
                description: Some(build_openclaw_description(
                    source_provider,
                    canonical_provider,
                    &model_key,
                    provider.api.as_deref(),
                    model,
                    enabled,
                    allowed_model_keys.is_some(),
                )),
                enabled,
            });
        }
    }

    if let Some(allowed_model_keys) = allowed_model_keys {
        let mut missing_allowed = allowed_model_keys.iter().cloned().collect::<Vec<_>>();
        missing_allowed.sort();

        for model_key in missing_allowed {
            if seen_model_keys.contains(&model_key) {
                continue;
            }

            let Some((provider, provider_ref)) = model_key.split_once('/') else {
                continue;
            };

            seen_model_keys.insert(model_key.clone());
            records.push(CapabilityRecord {
                capability_id: format!(
                    "cap.openclaw.model.{}.{}",
                    sanitize_capability_segment(provider),
                    sanitize_capability_segment(provider_ref)
                ),
                kind: "model".to_string(),
                provider: provider.to_string(),
                provider_ref: provider_ref.to_string(),
                display_name: format!("OpenClaw {provider_ref}"),
                version: "openclaw-local".to_string(),
                description: Some(format!(
                    "Imported from local OpenClaw allowlist as {model_key}; detailed model metadata was not present in local models.json"
                )),
                enabled: true,
            });
        }
    }

    records
}

fn build_openclaw_description(
    source_provider: &str,
    canonical_provider: &str,
    model_key: &str,
    api: Option<&str>,
    model: &OpenClawModelEntry,
    enabled: bool,
    allowlist_known: bool,
) -> String {
    let mut parts = vec![format!(
        "Imported from local OpenClaw model catalog as {model_key}"
    )];

    if source_provider != canonical_provider {
        parts.push(format!("source provider {source_provider}"));
    }
    if let Some(api) = api.filter(|value| !value.trim().is_empty()) {
        parts.push(format!("api {api}"));
    }
    if let Some(context_window) = model.context_window {
        parts.push(format!("context {context_window}"));
    }
    if let Some(max_tokens) = model.max_tokens {
        parts.push(format!("max tokens {max_tokens}"));
    }
    if model.reasoning.unwrap_or(false) {
        parts.push("reasoning enabled".to_string());
    }
    if let Some(inputs) = model
        .input
        .as_ref()
        .filter(|inputs| !inputs.is_empty())
        .map(|inputs| inputs.join("+"))
    {
        parts.push(format!("input {inputs}"));
    }
    if allowlist_known {
        parts.push(if enabled {
            "currently enabled by local OpenClaw allowlist".to_string()
        } else {
            "currently discovered but not enabled by local OpenClaw allowlist".to_string()
        });
    }

    parts.join("; ")
}

fn canonical_openclaw_provider<'a>(provider: &'a str, api: Option<&str>) -> &'a str {
    match (provider, api.unwrap_or_default()) {
        ("codex", _) => "openai-codex",
        ("minimax-cn", _) => "minimax",
        (_, "openai-codex-responses") if provider != "openai-codex" => "openai-codex",
        _ => provider,
    }
}

fn infer_capability_kind(model: &OpenClawModelEntry) -> &'static str {
    let id = model.id.to_ascii_lowercase();
    let name = model
        .name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if id.contains("embed") || name.contains("embed") {
        "embedding"
    } else {
        "model"
    }
}

fn sanitize_capability_segment(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

fn expand_tilde(value: &str) -> PathBuf {
    if let Some(stripped) = value.strip_prefix("~/") {
        if let Some(home_dir) = env::var_os("HOME") {
            return PathBuf::from(home_dir).join(stripped);
        }
    }

    PathBuf::from(value)
}

fn apply_openclaw_scope_env(command: &mut Command) {
    for (key, value) in [
        (
            "OPENCLAW_CONFIG_PATH",
            env::var("OPENCLAW_CONFIG_PATH").ok(),
        ),
        ("OPENCLAW_STATE_DIR", env::var("OPENCLAW_STATE_DIR").ok()),
        ("OPENCLAW_AGENT_DIR", env::var("OPENCLAW_AGENT_DIR").ok()),
    ] {
        if let Some(value) = value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            command.env(key, value);
        }
    }
}

fn default_capabilities() -> Vec<CapabilityRecord> {
    vec![
        CapabilityRecord {
            capability_id: "cap.demo.summarize".to_string(),
            kind: "model".to_string(),
            provider: "demo".to_string(),
            provider_ref: "demo/summarize-v1".to_string(),
            display_name: "Demo Summarize".to_string(),
            version: "v1".to_string(),
            description: Some(
                "Local dev placeholder capability for summarize-style invocations".to_string(),
            ),
            enabled: true,
        },
        CapabilityRecord {
            capability_id: "cap.demo.publish-review".to_string(),
            kind: "workflow".to_string(),
            provider: "demo".to_string(),
            provider_ref: "demo/publish-review-v1".to_string(),
            display_name: "Demo Publish Review".to_string(),
            version: "v1".to_string(),
            description: Some(
                "Local dev placeholder capability for approval-gated publish-style flows"
                    .to_string(),
            ),
            enabled: true,
        },
    ]
}

#[derive(Debug, Deserialize)]
struct OpenClawModelsCatalog {
    providers: HashMap<String, OpenClawProviderEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenClawProviderEntry {
    api: Option<String>,
    #[serde(default)]
    models: Vec<OpenClawModelEntry>,
}

#[derive(Debug, Deserialize)]
struct OpenClawModelEntry {
    id: String,
    name: Option<String>,
    reasoning: Option<bool>,
    input: Option<Vec<String>>,
    #[serde(rename = "contextWindow")]
    context_window: Option<u64>,
    #[serde(rename = "maxTokens")]
    max_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct OpenClawModelsStatus {
    #[serde(default)]
    allowed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{
        build_openclaw_capabilities, infer_capability_kind, OpenClawModelEntry,
        OpenClawModelsCatalog, OpenClawProviderEntry,
    };

    fn sample_catalog() -> OpenClawModelsCatalog {
        OpenClawModelsCatalog {
            providers: HashMap::from([
                (
                    "codex".to_string(),
                    OpenClawProviderEntry {
                        api: Some("openai-codex-responses".to_string()),
                        models: vec![OpenClawModelEntry {
                            id: "gpt-5.4".to_string(),
                            name: Some("gpt-5.4".to_string()),
                            reasoning: Some(true),
                            input: Some(vec!["text".to_string(), "image".to_string()]),
                            context_window: Some(272000),
                            max_tokens: Some(128000),
                        }],
                    },
                ),
                (
                    "openai-codex".to_string(),
                    OpenClawProviderEntry {
                        api: Some("openai-codex-responses".to_string()),
                        models: Vec::new(),
                    },
                ),
                (
                    "minimax".to_string(),
                    OpenClawProviderEntry {
                        api: Some("anthropic-messages".to_string()),
                        models: vec![OpenClawModelEntry {
                            id: "MiniMax-M2.5".to_string(),
                            name: Some("MiniMax M2.5".to_string()),
                            reasoning: Some(true),
                            input: Some(vec!["text".to_string()]),
                            context_window: Some(200000),
                            max_tokens: Some(8192),
                        }],
                    },
                ),
                (
                    "minimax-cn".to_string(),
                    OpenClawProviderEntry {
                        api: Some("anthropic-messages".to_string()),
                        models: vec![OpenClawModelEntry {
                            id: "MiniMax-M2.5".to_string(),
                            name: Some("MiniMax M2.5 CN".to_string()),
                            reasoning: Some(true),
                            input: Some(vec!["text".to_string()]),
                            context_window: Some(200000),
                            max_tokens: Some(8192),
                        }],
                    },
                ),
                (
                    "ollama".to_string(),
                    OpenClawProviderEntry {
                        api: Some("ollama".to_string()),
                        models: vec![OpenClawModelEntry {
                            id: "mxbai-embed-large:latest".to_string(),
                            name: Some("mxbai-embed-large:latest".to_string()),
                            reasoning: Some(false),
                            input: Some(vec!["text".to_string()]),
                            context_window: Some(512),
                            max_tokens: Some(8192),
                        }],
                    },
                ),
            ]),
        }
    }

    #[test]
    fn build_openclaw_capabilities_normalizes_aliases_and_allowlist() {
        let allowed = HashSet::from([
            "openai-codex/gpt-5.4".to_string(),
            "openai-codex/gpt-5.5".to_string(),
        ]);
        let records = build_openclaw_capabilities(&sample_catalog(), Some(&allowed));

        assert_eq!(records.len(), 4);

        let codex = records
            .iter()
            .find(|record| record.provider == "openai-codex")
            .expect("openai-codex record");
        assert_eq!(codex.provider_ref, "gpt-5.4");
        assert!(codex.enabled);
        assert!(codex
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("source provider codex"));

        let minimax = records
            .iter()
            .find(|record| record.provider == "minimax" && record.provider_ref == "MiniMax-M2.5")
            .expect("minimax record");
        assert!(!minimax.enabled);

        let embedding = records
            .iter()
            .find(|record| record.provider == "ollama")
            .expect("ollama record");
        assert_eq!(embedding.kind, "embedding");

        let missing_allowed = records
            .iter()
            .find(|record| record.provider == "openai-codex" && record.provider_ref == "gpt-5.5")
            .expect("missing allowed record");
        assert!(missing_allowed.enabled);
        assert!(missing_allowed
            .description
            .as_deref()
            .unwrap_or_default()
            .contains("not present in local models.json"));
    }

    #[test]
    fn infer_capability_kind_detects_embeddings() {
        assert_eq!(
            infer_capability_kind(&OpenClawModelEntry {
                id: "nomic-embed-text:latest".to_string(),
                name: Some("Nomic Embed Text".to_string()),
                reasoning: None,
                input: None,
                context_window: None,
                max_tokens: None,
            }),
            "embedding"
        );
        assert_eq!(
            infer_capability_kind(&OpenClawModelEntry {
                id: "gpt-5.4".to_string(),
                name: Some("GPT 5.4".to_string()),
                reasoning: None,
                input: None,
                context_window: None,
                max_tokens: None,
            }),
            "model"
        );
    }
}
