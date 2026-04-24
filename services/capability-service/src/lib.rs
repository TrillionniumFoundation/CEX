use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use shared_types::CapabilityRecord;
use std::{collections::HashMap, env, sync::Arc};

#[derive(Clone)]
pub struct AppState {
    capabilities: Arc<HashMap<String, CapabilityRecord>>,
}

impl AppState {
    pub async fn from_env() -> Self {
        Self {
            capabilities: Arc::new(load_capabilities()),
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
        .route("/v1/capabilities", get(list_capabilities))
        .route("/v1/capabilities/:id", get(get_capability))
        .with_state(state)
}

async fn health() -> &'static str {
    "capability-service ok"
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

fn load_capabilities() -> HashMap<String, CapabilityRecord> {
    let records = env::var("CAPABILITY_STATIC_REGISTRY_JSON")
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<CapabilityRecord>>(&raw).ok())
        .filter(|records| !records.is_empty())
        .unwrap_or_else(default_capabilities);

    records
        .into_iter()
        .map(|record| (record.capability_id.clone(), record))
        .collect()
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

use axum::response::IntoResponse;
