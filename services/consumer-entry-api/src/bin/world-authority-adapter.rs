use axum::{
    body::{to_bytes, Bytes},
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde_json::{json, Value};
use shared_tracing::init_tracing;
use std::{env, net::SocketAddr, time::Duration};

const ADAPTER_CONTRACT: &str = "cex_trillionnium_world_authority_adapter_v1";
const WORLD_API_CONTRACT: &str = "trillionnium_world_api_v1";
const WORLD_CUTOVER_CONTRACT: &str = "trillionnium_world_authority_cutover_v1";
const MAX_PROXY_BODY_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Clone)]
struct AdapterState {
    client: Client,
    base_url: String,
    auth_token: Option<String>,
    production_like: bool,
}

#[derive(Debug, Clone)]
struct AdapterConfig {
    bind_addr: SocketAddr,
    base_url: String,
    auth_token: Option<String>,
    timeout_ms: u64,
    production_like: bool,
}

fn first_non_empty_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn profile_is_production_like(profile: Option<&str>) -> bool {
    profile
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .map(|value| {
            matches!(
                value.as_str(),
                "beta" | "staging" | "stage" | "production" | "prod"
            )
        })
        .unwrap_or(false)
}

fn weak_secret(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.len() < 24
        || normalized.contains("change-me")
        || normalized.contains("replace_me")
        || normalized.contains("password")
        || normalized.contains("example")
        || normalized == "secret"
}

fn valid_world_base_url(value: &str, production_like: bool) -> bool {
    let normalized = value.trim().trim_end_matches('/').to_ascii_lowercase();
    let scheme_ok = normalized.starts_with("https://") || normalized.starts_with("http://");
    let placeholder = normalized.contains("change-me")
        || normalized.contains("replace_me")
        || normalized.contains("example.com");
    let loopback = normalized.contains("localhost")
        || normalized.contains("127.0.0.1")
        || normalized.contains("0.0.0.0");
    scheme_ok && !placeholder && (!production_like || !loopback)
}

impl AdapterConfig {
    fn from_env() -> Result<Self, String> {
        let profile = first_non_empty_env(&[
            "WORLD_AUTHORITY_ADAPTER_RUNTIME_PROFILE",
            "CONSUMER_ENTRY_RUNTIME_PROFILE",
            "CEX_RUNTIME_PROFILE",
            "APP_ENV",
        ]);
        let production_like = profile_is_production_like(profile.as_deref());
        let bind_addr = first_non_empty_env(&["WORLD_AUTHORITY_ADAPTER_BIND_ADDR"])
            .unwrap_or_else(|| "127.0.0.1:8096".to_string())
            .parse::<SocketAddr>()
            .map_err(|error| format!("invalid WORLD_AUTHORITY_ADAPTER_BIND_ADDR: {error}"))?;
        let base_url = first_non_empty_env(&["TRILLIONNIUM_WORLD_BASE_URL"])
            .unwrap_or_else(|| "http://127.0.0.1:8787".to_string())
            .trim_end_matches('/')
            .to_string();
        if !valid_world_base_url(&base_url, production_like) {
            return Err(
                "TRILLIONNIUM_WORLD_BASE_URL must be a non-placeholder HTTP(S) URL; production-like profiles reject loopback"
                    .to_string(),
            );
        }

        let api_contract = first_non_empty_env(&["TRILLIONNIUM_WORLD_API_CONTRACT"])
            .unwrap_or_else(|| WORLD_API_CONTRACT.to_string());
        if api_contract != WORLD_API_CONTRACT {
            return Err(format!(
                "TRILLIONNIUM_WORLD_API_CONTRACT must equal {WORLD_API_CONTRACT}, got {api_contract}"
            ));
        }

        let auth_token = first_non_empty_env(&["TRILLIONNIUM_WORLD_AUTH_TOKEN"]);
        if production_like {
            let token = auth_token.as_deref().ok_or_else(|| {
                "TRILLIONNIUM_WORLD_AUTH_TOKEN is required in production-like profiles".to_string()
            })?;
            if weak_secret(token) {
                return Err(
                    "TRILLIONNIUM_WORLD_AUTH_TOKEN is weak or placeholder-like".to_string(),
                );
            }
        }

        let timeout_ms = first_non_empty_env(&["WORLD_AUTHORITY_ADAPTER_TIMEOUT_MS"])
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|error| format!("invalid WORLD_AUTHORITY_ADAPTER_TIMEOUT_MS: {error}"))
            })
            .transpose()?
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(250..=30_000).contains(&timeout_ms) {
            return Err(
                "WORLD_AUTHORITY_ADAPTER_TIMEOUT_MS must be between 250 and 30000"
                    .to_string(),
            );
        }

        Ok(Self {
            bind_addr,
            base_url,
            auth_token,
            timeout_ms,
            production_like,
        })
    }

    fn into_state(self) -> Result<AdapterState, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_millis(self.timeout_ms.min(3_000)))
            .timeout(Duration::from_millis(self.timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("failed to build World authority HTTP client: {error}"))?;
        Ok(AdapterState {
            client,
            base_url: self.base_url,
            auth_token: self.auth_token,
            production_like: self.production_like,
        })
    }
}

fn upstream_path(path: &str) -> Option<String> {
    if path == "/health" {
        return Some("/health".to_string());
    }
    let suffix = path.strip_prefix("/v1/world")?;
    if suffix.is_empty() || suffix == "/" {
        return Some("/world/full-split".to_string());
    }
    if !suffix.starts_with('/') || suffix.contains("..") || suffix.contains("//") {
        return None;
    }
    Some(format!("/world{suffix}"))
}

fn copy_request_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "content-type"
            | "accept"
            | "idempotency-key"
            | "traceparent"
            | "tracestate"
            | "x-request-id"
            | "x-cex-user-session"
            | "x-cex-user-session-signature"
            | "x-cex-product-user-id"
            | "x-cex-account-id"
            | "x-cex-org-id"
            | "x-cex-room-id"
    )
}

fn copy_response_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "content-type"
            | "cache-control"
            | "etag"
            | "retry-after"
            | "x-request-id"
            | "x-trillionnium-world-api-contract"
            | "x-trillionnium-world-state-version"
    )
}

fn json_contract_is_compatible(bytes: &Bytes) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    let serialized = value.to_string();
    serialized.contains(WORLD_API_CONTRACT)
        || serialized.contains("trillionnium_world_domain_v1")
        || serialized.contains(WORLD_CUTOVER_CONTRACT)
}

async fn local_health(State(state): State<AdapterState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "adapter_contract": ADAPTER_CONTRACT,
        "world_api_contract": WORLD_API_CONTRACT,
        "cutover_contract": WORLD_CUTOVER_CONTRACT,
        "authority_mode": "remote_only",
        "local_world_writer": false,
        "production_like": state.production_like,
        "upstream_configured": true,
        "production_authorization": "not_granted_until_cross_repository_evidence_is_exact"
    }))
}

async fn proxy_world(State(state): State<AdapterState>, request: Request) -> Response {
    let Some(path) = upstream_path(request.uri().path()) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "unsupported_world_authority_route",
                "adapter_contract": ADAPTER_CONTRACT
            })),
        )
            .into_response();
    };

    let query = request
        .uri()
        .query()
        .map(|value| format!("?{value}"))
        .unwrap_or_default();
    let target = format!("{}{}{}", state.base_url, path, query);
    let method = request.method().clone();
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), MAX_PROXY_BODY_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({
                    "error": "world_authority_request_body_rejected",
                    "reason": error.to_string(),
                    "adapter_contract": ADAPTER_CONTRACT
                })),
            )
                .into_response();
        }
    };

    let mut upstream = state.client.request(method, target);
    for (name, value) in &headers {
        if copy_request_header(name) {
            upstream = upstream.header(name, value);
        }
    }
    upstream = upstream
        .header("x-trillionnium-world-api-contract", WORLD_API_CONTRACT)
        .header("x-trillionnium-world-cutover-contract", WORLD_CUTOVER_CONTRACT)
        .header("x-cex-world-adapter-contract", ADAPTER_CONTRACT)
        .body(body);
    if let Some(token) = state.auth_token.as_deref() {
        upstream = upstream.bearer_auth(token);
    }

    let upstream = match upstream.send().await {
        Ok(response) => response,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "world_authority_unavailable",
                    "reason": error.to_string(),
                    "adapter_contract": ADAPTER_CONTRACT,
                    "retryable": true,
                    "local_fallback_used": false
                })),
            )
                .into_response();
        }
    };

    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let bytes = match upstream.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "error": "world_authority_response_body_failed",
                    "reason": error.to_string(),
                    "adapter_contract": ADAPTER_CONTRACT,
                    "local_fallback_used": false
                })),
            )
                .into_response();
        }
    };

    if state.production_like
        && status.is_success()
        && path != "/health"
        && !json_contract_is_compatible(&bytes)
    {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": "world_authority_contract_mismatch",
                "expected": WORLD_API_CONTRACT,
                "adapter_contract": ADAPTER_CONTRACT,
                "local_fallback_used": false
            })),
        )
            .into_response();
    }

    let mut response = (status, bytes).into_response();
    for (name, value) in &upstream_headers {
        if copy_response_header(name) {
            response.headers_mut().insert(name.clone(), value.clone());
        }
    }
    response.headers_mut().insert(
        HeaderName::from_static("x-cex-world-adapter-contract"),
        HeaderValue::from_static(ADAPTER_CONTRACT),
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-trillionnium-world-api-contract"),
        HeaderValue::from_static(WORLD_API_CONTRACT),
    );
    response.headers_mut().insert(
        header::VARY,
        HeaderValue::from_static("Authorization, X-CEX-User-Session"),
    );
    response
}

#[tokio::main]
async fn main() {
    init_tracing();
    let config = AdapterConfig::from_env().unwrap_or_else(|error| {
        eprintln!("World authority adapter startup guard rejected configuration: {error}");
        std::process::exit(78);
    });
    let bind_addr = config.bind_addr;
    let state = config.into_state().unwrap_or_else(|error| {
        eprintln!("World authority adapter startup failed: {error}");
        std::process::exit(78);
    });
    let app = Router::new()
        .route("/health", get(local_health))
        .fallback(proxy_world)
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .unwrap_or_else(|error| {
            eprintln!("failed to bind World authority adapter at {bind_addr}: {error}");
            std::process::exit(70);
        });
    axum::serve(listener, app).await.unwrap_or_else(|error| {
        eprintln!("World authority adapter server failed: {error}");
        std::process::exit(70);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_mapping_is_versioned_and_traversal_safe() {
        assert_eq!(
            upstream_path("/v1/world/command").as_deref(),
            Some("/world/command")
        );
        assert_eq!(
            upstream_path("/v1/world/full-split").as_deref(),
            Some("/world/full-split")
        );
        assert!(upstream_path("/v1/world/../admin").is_none());
        assert!(upstream_path("/unrelated").is_none());
    }

    #[test]
    fn production_url_and_secret_checks_are_fail_closed() {
        assert!(valid_world_base_url("https://world.internal:8787", true));
        assert!(!valid_world_base_url("http://127.0.0.1:8787", true));
        assert!(valid_world_base_url("http://127.0.0.1:8787", false));
        assert!(weak_secret("change-me"));
        assert!(!weak_secret("Qm9uZGVkLXdvcmxkLWF1dGhvcml0eS10b2tlbg"));
    }

    #[test]
    fn compatible_json_requires_a_world_contract() {
        let compatible =
            Bytes::from_static(br#"{"api_contract":"trillionnium_world_api_v1"}"#);
        let incompatible = Bytes::from_static(br#"{"status":"ok"}"#);
        assert!(json_contract_is_compatible(&compatible));
        assert!(!json_contract_is_compatible(&incompatible));
    }
}
