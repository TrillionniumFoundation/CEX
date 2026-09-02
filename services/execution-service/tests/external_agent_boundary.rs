use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use execution_service::{
    build_router,
    providers::{
        build_provider_target, dispatch_via_provider, parse_provider_target,
        OpenClawCliEnvScope, ProviderDispatchInput, LEGACY_LOCAL_DISPATCH_STATUS, RUNTIME_POLICY,
    },
    state::AppState,
};
use reqwest::Client;
use tower::util::ServiceExt;

fn test_state() -> AppState {
    AppState::new_for_tests(
        false,
        Some("local-dev-admin-token".to_string()),
        vec![
            "executions:manage".to_string(),
            "executions:read".to_string(),
        ],
        Vec::new(),
    )
}

#[tokio::test]
async fn default_execution_router_remains_live_without_local_provider_runtime() {
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/health")
                .body(Body::empty())
                .expect("build health request"),
        )
        .await
        .expect("health response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 65_536)
        .await
        .expect("bounded health body");
    assert!(!body.is_empty());
}

#[tokio::test]
async fn default_execution_router_does_not_expose_retired_process_endpoint() {
    let response = build_router(test_state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/executions/00000000-0000-0000-0000-000000000000/process")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .expect("build retired process request"),
        )
        .await
        .expect("retired process response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn default_router_source_cannot_regain_the_retired_process_route_silently() {
    let source = include_str!("../src/lib.rs");
    assert!(!source.contains("/v1/executions/:id/process"));
    assert!(!source.contains("post(api::process_execution)"));
}

#[tokio::test]
async fn provider_compatibility_surface_fails_closed_without_network_or_prompt_echo() {
    let prompt = "TOP-SECRET-RESEARCH-PROMPT";
    let error = dispatch_via_provider(
        &Client::new(),
        "http://127.0.0.1:9",
        "/tmp/nonexistent-openclaw",
        &OpenClawCliEnvScope::default(),
        1,
        "ollama://historical-model",
        &ProviderDispatchInput {
            prompt: prompt.to_string(),
        },
    )
    .await
    .expect_err("default CEX runtime must never execute a local provider");

    assert_eq!(RUNTIME_POLICY, "external_only");
    assert_eq!(
        LEGACY_LOCAL_DISPATCH_STATUS,
        "legacy_local_provider_dispatch_disabled"
    );
    assert!(error.message.contains("external_agent_runtime_required"));
    assert!(error.message.contains("hepta_agent_protocol_v1"));
    assert!(!error.message.contains(prompt));
    assert!(!error.message.contains("127.0.0.1"));
    assert!(!error.message.contains("historical-model"));
}

#[test]
fn provider_identity_helpers_are_pure_and_bounded_by_canonical_shape() {
    let target = build_provider_target("external-agent", "did:trnm:agent-alpha");
    assert_eq!(target, "external-agent://did:trnm:agent-alpha");
    assert_eq!(
        parse_provider_target(&target),
        Some(("external-agent", "did:trnm:agent-alpha"))
    );
    assert_eq!(parse_provider_target("external-agent://"), None);
    assert_eq!(parse_provider_target(" external-agent://did:trnm:a"), None);
    assert_eq!(parse_provider_target("external-agent://did:trnm:a\nsecret"), None);
}
