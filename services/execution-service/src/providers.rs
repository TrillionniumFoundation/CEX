use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RUNTIME_POLICY: &str = "external_only";
pub const LEGACY_LOCAL_DISPATCH_STATUS: &str = "legacy_local_provider_dispatch_disabled";
const EXTERNAL_AGENT_REQUIRED: &str = "external_agent_runtime_required: CEX does not execute participating Agents; submit signed hepta_agent_protocol_v1 evidence through the external Agent boundary";

/// Retained only so historical provider-dispatch rows and the explicitly
/// feature-gated compatibility worker keep one stable source contract. The
/// fields are never interpreted by the default runtime.
#[derive(Debug, Clone, Default)]
pub struct OpenClawCliEnvScope {
    pub config_path: Option<String>,
    pub state_dir: Option<String>,
    pub agent_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDispatchInput {
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDispatchOutput {
    pub provider: String,
    pub provider_ref: String,
    pub provider_target: String,
    pub result_payload: Value,
}

#[derive(Debug, Clone)]
pub struct ProviderDispatchError {
    pub message: String,
}

impl ProviderDispatchError {
    fn external_agent_required() -> Self {
        Self {
            message: EXTERNAL_AGENT_REQUIRED.to_string(),
        }
    }

    fn invalid_target() -> Self {
        Self {
            message: "invalid_provider_target: provider target must use a non-empty provider://reference identity"
                .to_string(),
        }
    }
}

/// Fail-closed compatibility surface.
///
/// Sequence 51 removes all local Ollama/OpenClaw execution code from CEX. This
/// function deliberately preserves the old call signature so historical
/// lifecycle and reconciliation code can compile while every attempted local
/// dispatch produces a stable, non-sensitive error. It never reads the prompt,
/// starts a child process, performs an inference HTTP request, or copies a
/// provider body into logs/database error text.
pub async fn dispatch_via_provider(
    _http: &Client,
    _ollama_base_url: &str,
    _openclaw_cli_bin: &str,
    _openclaw_env_scope: &OpenClawCliEnvScope,
    _provider_timeout_seconds: u64,
    provider_target: &str,
    _input: &ProviderDispatchInput,
) -> Result<ProviderDispatchOutput, ProviderDispatchError> {
    parse_provider_target(provider_target).ok_or_else(ProviderDispatchError::invalid_target)?;
    Err(ProviderDispatchError::external_agent_required())
}

pub fn build_provider_target(provider: &str, provider_ref: &str) -> String {
    format!("{provider}://{provider_ref}")
}

pub fn parse_provider_target(provider_target: &str) -> Option<(&str, &str)> {
    let (provider, provider_ref) = provider_target.split_once("://")?;
    if provider.is_empty()
        || provider_ref.is_empty()
        || provider.trim() != provider
        || provider_ref.trim() != provider_ref
        || provider.chars().any(char::is_control)
        || provider_ref.chars().any(char::is_control)
    {
        return None;
    }
    Some((provider, provider_ref))
}

#[cfg(test)]
mod tests {
    use super::{
        build_provider_target, dispatch_via_provider, parse_provider_target,
        OpenClawCliEnvScope, ProviderDispatchInput, LEGACY_LOCAL_DISPATCH_STATUS, RUNTIME_POLICY,
    };
    use reqwest::Client;

    #[test]
    fn provider_target_round_trip_preserves_identity() {
        let target = build_provider_target("external-agent", "did:trnm:agent-alpha");
        assert_eq!(
            parse_provider_target(&target),
            Some(("external-agent", "did:trnm:agent-alpha"))
        );
        assert_eq!(RUNTIME_POLICY, "external_only");
        assert_eq!(
            LEGACY_LOCAL_DISPATCH_STATUS,
            "legacy_local_provider_dispatch_disabled"
        );
    }

    #[test]
    fn malformed_provider_targets_fail_closed() {
        assert_eq!(parse_provider_target("ollama"), None);
        assert_eq!(parse_provider_target("://model"), None);
        assert_eq!(parse_provider_target("ollama://"), None);
        assert_eq!(parse_provider_target(" ollama://model"), None);
        assert_eq!(parse_provider_target("ollama://model\nsecret"), None);
    }

    #[tokio::test]
    async fn dispatch_never_executes_a_local_provider_or_echoes_prompt() {
        let secret_prompt = "PRIVATE-PROMPT-MUST-NOT-APPEAR";
        let error = dispatch_via_provider(
            &Client::new(),
            "http://127.0.0.1:11434",
            "openclaw",
            &OpenClawCliEnvScope::default(),
            1,
            "ollama://demo",
            &ProviderDispatchInput {
                prompt: secret_prompt.to_string(),
            },
        )
        .await
        .expect_err("local provider execution must be disabled");

        assert!(error.message.contains("external_agent_runtime_required"));
        assert!(error.message.contains("hepta_agent_protocol_v1"));
        assert!(!error.message.contains(secret_prompt));
        assert!(!error.message.contains("127.0.0.1"));
        assert!(!error.message.contains("demo"));
    }
}
