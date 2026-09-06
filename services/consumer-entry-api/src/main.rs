use axum::{
    extract::Request,
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Json, Router,
};
use consumer_entry_api::{build_router, AppState};
use serde_json::json;
use shared_tracing::init_tracing;
use std::env;

const WORLD_AUTHORITY_CONTRACT: &str = "trillionnium_world_authority_cutover_v1";
const WORLD_API_CONTRACT: &str = "trillionnium_world_api_v1";

fn first_non_empty_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn production_like_profile() -> bool {
    first_non_empty_env(&["CONSUMER_ENTRY_RUNTIME_PROFILE", "CEX_RUNTIME_PROFILE", "APP_ENV"])
        .map(|profile| {
            matches!(
                profile.trim().to_ascii_lowercase().as_str(),
                "beta" | "staging" | "stage" | "production" | "prod"
            )
        })
        .unwrap_or(false)
}

fn valid_remote_world_base_url(value: &str) -> bool {
    let normalized = value.trim().trim_end_matches('/').to_ascii_lowercase();
    (normalized.starts_with("https://") || normalized.starts_with("http://"))
        && !normalized.contains("change-me")
        && !normalized.contains("replace_me")
        && !normalized.contains("example.com")
        && !normalized.contains("localhost")
        && !normalized.contains("127.0.0.1")
        && !normalized.contains("0.0.0.0")
}

fn validate_world_authority_startup_from_env() -> Result<(), String> {
    if !production_like_profile() {
        return Ok(());
    }

    let mode = first_non_empty_env(&["CEX_WORLD_AUTHORITY_MODE"])
        .unwrap_or_else(|| "embedded".to_string())
        .to_ascii_lowercase();
    if mode != "remote" {
        return Err(format!(
            "production-like consumer-entry requires CEX_WORLD_AUTHORITY_MODE=remote; local World authority is quarantined by {WORLD_AUTHORITY_CONTRACT}"
        ));
    }

    let base_url = first_non_empty_env(&["TRILLIONNIUM_WORLD_BASE_URL"])
        .ok_or_else(|| "TRILLIONNIUM_WORLD_BASE_URL is required in remote World authority mode".to_string())?;
    if !valid_remote_world_base_url(&base_url) {
        return Err(
            "TRILLIONNIUM_WORLD_BASE_URL must be a non-placeholder, non-loopback HTTP(S) service URL"
                .to_string(),
        );
    }

    let api_contract = first_non_empty_env(&["TRILLIONNIUM_WORLD_API_CONTRACT"])
        .ok_or_else(|| "TRILLIONNIUM_WORLD_API_CONTRACT is required in production-like profiles".to_string())?;
    if api_contract != WORLD_API_CONTRACT {
        return Err(format!(
            "TRILLIONNIUM_WORLD_API_CONTRACT must equal {WORLD_API_CONTRACT}, got {api_contract}"
        ));
    }

    Ok(())
}

fn is_world_authoritative_write(method: &Method, path: &str) -> bool {
    let write_method = matches!(method.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
    if !write_method {
        return false;
    }
    if !path.starts_with("/world") {
        return false;
    }
    if path.ends_with("/map-rum") {
        return false;
    }

    const AUTHORITY_MARKERS: &[&str] = &[
        "/action",
        "/move",
        "/movement",
        "/tactics",
        "/asset-upgrade",
        "/company",
        "/shop",
        "/listing",
        "/buy",
        "/contract-complete",
        "/work-deliver",
        "/work-accept",
        "/work-reject",
        "/work-reopen",
        "/work-cancel",
        "/route-command",
    ];
    AUTHORITY_MARKERS.iter().any(|marker| path.contains(marker))
}

async fn world_authority_write_fence(request: Request, next: Next) -> Response {
    if production_like_profile()
        && is_world_authoritative_write(request.method(), request.uri().path())
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "remote_world_authority_required",
                "status": "local_world_writer_quarantined",
                "authority_contract": WORLD_AUTHORITY_CONTRACT,
                "api_contract": WORLD_API_CONTRACT,
                "retryable": true,
                "source_of_truth": "TrillionniumFoundation/Trillionnium-World",
            })),
        )
            .into_response();
    }
    next.run(request).await
}

#[tokio::main]
async fn main() {
    init_tracing();

    if let Err(err) = validate_world_authority_startup_from_env() {
        eprintln!("consumer-entry World authority startup guard rejected configuration: {err}");
        std::process::exit(78);
    }

    let state = match AppState::from_env().await {
        Ok(state) => state,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };
    let app: Router =
        build_router(state.clone()).layer(middleware::from_fn(world_authority_write_fence));

    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_world_authority_writes_are_classified() {
        for path in [
            "/world/web/action",
            "/world/web/company",
            "/world/web/listing",
            "/world/web/work-deliver",
            "/world/local-player/move",
            "/world/tactics-command",
        ] {
            assert!(is_world_authoritative_write(&Method::POST, path), "{path}");
        }
    }

    #[test]
    fn read_and_telemetry_routes_are_not_classified_as_authority_writes() {
        assert!(!is_world_authoritative_write(
            &Method::GET,
            "/world/local-player/map"
        ));
        assert!(!is_world_authoritative_write(
            &Method::POST,
            "/world/web/map-rum"
        ));
        assert!(!is_world_authoritative_write(
            &Method::POST,
            "/v1/tasks"
        ));
    }

    #[test]
    fn production_remote_url_rejects_placeholders_and_loopback() {
        assert!(valid_remote_world_base_url("https://world.internal:8787"));
        assert!(!valid_remote_world_base_url("http://127.0.0.1:8787"));
        assert!(!valid_remote_world_base_url("https://example.com"));
        assert!(!valid_remote_world_base_url("change-me"));
    }
}
