use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
    fn transport(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn upstream(status: u16, body: impl Into<String>) -> Self {
        Self {
            message: format!(
                "provider upstream returned status {status}: {}",
                body.into()
            ),
        }
    }

    fn decode(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn provider(&self) -> &'static str;

    async fn execute(
        &self,
        http: &Client,
        provider_ref: &str,
        input: &ProviderDispatchInput,
    ) -> Result<ProviderDispatchOutput, ProviderDispatchError>;
}

pub struct OllamaProviderAdapter {
    pub base_url: String,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    model: Option<String>,
    response: Option<String>,
    done: Option<bool>,
    total_duration: Option<u64>,
    eval_count: Option<u64>,
    prompt_eval_count: Option<u64>,
}

#[async_trait]
impl ProviderAdapter for OllamaProviderAdapter {
    fn provider(&self) -> &'static str {
        "ollama"
    }

    async fn execute(
        &self,
        http: &Client,
        provider_ref: &str,
        input: &ProviderDispatchInput,
    ) -> Result<ProviderDispatchOutput, ProviderDispatchError> {
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let response = http
            .post(url)
            .json(&json!({
                "model": provider_ref,
                "prompt": input.prompt,
                "stream": false,
            }))
            .send()
            .await
            .map_err(|err| ProviderDispatchError::transport(format!("request failed: {err}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|err| ProviderDispatchError::transport(format!("read body failed: {err}")))?;

        if !status.is_success() {
            return Err(ProviderDispatchError::upstream(status.as_u16(), body));
        }

        let parsed: OllamaGenerateResponse = serde_json::from_str(&body).map_err(|err| {
            ProviderDispatchError::decode(format!("decode response failed: {err}; body={body}"))
        })?;

        Ok(ProviderDispatchOutput {
            provider: self.provider().to_string(),
            provider_ref: provider_ref.to_string(),
            provider_target: build_provider_target(self.provider(), provider_ref),
            result_payload: json!({
                "provider": self.provider(),
                "provider_ref": provider_ref,
                "model": parsed.model,
                "output_text": parsed.response,
                "done": parsed.done,
                "total_duration": parsed.total_duration,
                "eval_count": parsed.eval_count,
                "prompt_eval_count": parsed.prompt_eval_count,
            }),
        })
    }
}

pub async fn dispatch_via_provider(
    http: &Client,
    ollama_base_url: &str,
    provider_target: &str,
    input: &ProviderDispatchInput,
) -> Result<ProviderDispatchOutput, ProviderDispatchError> {
    let (provider, provider_ref) = parse_provider_target(provider_target).ok_or_else(|| {
        ProviderDispatchError::decode(format!("invalid provider target: {provider_target}"))
    })?;

    match provider {
        "ollama" => {
            let adapter = OllamaProviderAdapter {
                base_url: ollama_base_url.to_string(),
            };
            adapter.execute(http, provider_ref, input).await
        }
        other => Err(ProviderDispatchError::decode(format!(
            "unsupported provider adapter: {other}"
        ))),
    }
}

pub fn build_provider_target(provider: &str, provider_ref: &str) -> String {
    format!("{provider}://{provider_ref}")
}

pub fn parse_provider_target(provider_target: &str) -> Option<(&str, &str)> {
    provider_target.split_once("://")
}
