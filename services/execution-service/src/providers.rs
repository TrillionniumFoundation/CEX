use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time::timeout;

#[derive(Debug, Clone, Default)]
pub struct OpenClawCliEnvScope {
    pub config_path: Option<String>,
    pub state_dir: Option<String>,
    pub agent_dir: Option<String>,
}

impl OpenClawCliEnvScope {
    fn apply_to_command(&self, command: &mut Command) {
        if let Some(config_path) = self.config_path.as_deref() {
            command.env("OPENCLAW_CONFIG_PATH", config_path);
        }
        if let Some(state_dir) = self.state_dir.as_deref() {
            command.env("OPENCLAW_STATE_DIR", state_dir);
        }
        if let Some(agent_dir) = self.agent_dir.as_deref() {
            command.env("OPENCLAW_AGENT_DIR", agent_dir);
        }
    }
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
    pub timeout_seconds: u64,
}

pub struct OpenClawCliProviderAdapter {
    pub cli_bin: String,
    pub source_provider: String,
    pub env_scope: OpenClawCliEnvScope,
    pub timeout_seconds: u64,
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

#[derive(Debug, Deserialize)]
struct OpenClawModelRunResponse {
    ok: Option<bool>,
    capability: Option<String>,
    transport: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    #[serde(default)]
    attempts: Vec<Value>,
    #[serde(default)]
    outputs: Vec<OpenClawModelRunOutput>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenClawModelRunOutput {
    text: Option<String>,
    #[serde(rename = "mediaUrl")]
    media_url: Option<String>,
}

enum OpenClawStdoutResult {
    Parsed(OpenClawModelRunResponse),
    Finished(Vec<u8>),
    ReadError(String),
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
        timeout(Duration::from_secs(self.timeout_seconds), async {
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
                .map_err(|err| {
                    ProviderDispatchError::transport(format!("request failed: {err}"))
                })?;

            let status = response.status();
            let body = response.text().await.map_err(|err| {
                ProviderDispatchError::transport(format!("read body failed: {err}"))
            })?;

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
        })
        .await
        .map_err(|_| {
            ProviderDispatchError::transport(format!(
                "provider dispatch timed out after {}s: {}",
                self.timeout_seconds,
                build_provider_target(self.provider(), provider_ref)
            ))
        })?
    }
}

#[async_trait]
impl ProviderAdapter for OpenClawCliProviderAdapter {
    fn provider(&self) -> &'static str {
        "openclaw-cli"
    }

    async fn execute(
        &self,
        _http: &Client,
        provider_ref: &str,
        input: &ProviderDispatchInput,
    ) -> Result<ProviderDispatchOutput, ProviderDispatchError> {
        let canonical_provider = canonical_openclaw_provider(&self.source_provider);
        let model_key = format!("{canonical_provider}/{provider_ref}");
        let mut command = Command::new(&self.cli_bin);
        self.env_scope.apply_to_command(&mut command);
        command
            .args(["infer", "model", "run", "--local", "--json", "--model"])
            .arg(&model_key)
            .args(["--prompt"])
            .arg(&input.prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            ProviderDispatchError::transport(format!(
                "spawn openclaw model bridge failed for {model_key}: {err}"
            ))
        })?;

        let mut stdout = child.stdout.take().ok_or_else(|| {
            ProviderDispatchError::transport(format!(
                "openclaw model bridge missing stdout pipe for {model_key}"
            ))
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| {
            ProviderDispatchError::transport(format!(
                "openclaw model bridge missing stderr pipe for {model_key}"
            ))
        })?;

        let (stdout_tx, stdout_rx) = oneshot::channel();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                match stdout.read(&mut chunk).await {
                    Ok(0) => {
                        let _ = stdout_tx.send(OpenClawStdoutResult::Finished(buf));
                        break;
                    }
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if let Ok(parsed) = serde_json::from_slice::<OpenClawModelRunResponse>(&buf)
                        {
                            let _ = stdout_tx.send(OpenClawStdoutResult::Parsed(parsed));
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = stdout_tx.send(OpenClawStdoutResult::ReadError(err.to_string()));
                        break;
                    }
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            buf
        });

        let parsed = match timeout(Duration::from_secs(self.timeout_seconds), stdout_rx).await {
            Ok(Ok(OpenClawStdoutResult::Parsed(parsed))) => {
                terminate_child(&mut child).await;
                let _ = stderr_task.await;
                parsed
            }
            Ok(Ok(OpenClawStdoutResult::Finished(stdout_buf))) => {
                let status = child.wait().await.map_err(|err| {
                    ProviderDispatchError::transport(format!(
                        "wait openclaw model bridge failed for {model_key}: {err}"
                    ))
                })?;
                let stderr_buf = stderr_task.await.unwrap_or_default();
                if !status.success() {
                    let stderr = String::from_utf8_lossy(&stderr_buf).trim().to_string();
                    let stdout = String::from_utf8_lossy(&stdout_buf).trim().to_string();
                    let detail = if !stderr.is_empty() { stderr } else { stdout };
                    return Err(ProviderDispatchError::transport(format!(
                        "openclaw model bridge failed for {model_key}: {detail}"
                    )));
                }
                serde_json::from_slice(&stdout_buf).map_err(|err| {
                    ProviderDispatchError::decode(format!(
                        "decode openclaw model bridge response failed: {err}; stdout={}",
                        String::from_utf8_lossy(&stdout_buf)
                    ))
                })?
            }
            Ok(Ok(OpenClawStdoutResult::ReadError(err))) => {
                terminate_child(&mut child).await;
                let _ = stderr_task.await;
                return Err(ProviderDispatchError::transport(format!(
                    "read openclaw model bridge stdout failed for {model_key}: {err}"
                )));
            }
            Ok(Err(_)) => {
                terminate_child(&mut child).await;
                let _ = stderr_task.await;
                return Err(ProviderDispatchError::transport(format!(
                    "openclaw model bridge stdout channel closed for {model_key}"
                )));
            }
            Err(_) => {
                terminate_child(&mut child).await;
                let _ = stderr_task.await;
                return Err(ProviderDispatchError::transport(format!(
                    "provider dispatch timed out after {}s: {model_key}",
                    self.timeout_seconds
                )));
            }
        };

        let output_text = parsed
            .outputs
            .iter()
            .find_map(|entry| entry.text.clone())
            .or_else(|| {
                parsed
                    .outputs
                    .first()
                    .and_then(|entry| entry.media_url.clone())
            });

        Ok(ProviderDispatchOutput {
            provider: canonical_provider.to_string(),
            provider_ref: provider_ref.to_string(),
            provider_target: build_provider_target(canonical_provider, provider_ref),
            result_payload: json!({
                "provider": parsed.provider.unwrap_or_else(|| canonical_provider.to_string()),
                "provider_ref": provider_ref,
                "model": parsed.model.unwrap_or_else(|| provider_ref.to_string()),
                "capability": parsed.capability,
                "transport": parsed.transport,
                "ok": parsed.ok,
                "output_text": output_text,
                "outputs": parsed.outputs,
                "attempts": parsed.attempts,
                "bridge": "openclaw-cli"
            }),
        })
    }
}

async fn terminate_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

pub async fn dispatch_via_provider(
    http: &Client,
    ollama_base_url: &str,
    openclaw_cli_bin: &str,
    openclaw_env_scope: &OpenClawCliEnvScope,
    provider_timeout_seconds: u64,
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
                timeout_seconds: provider_timeout_seconds,
            };
            adapter.execute(http, provider_ref, input).await
        }
        other if !openclaw_cli_bin.trim().is_empty() => {
            let adapter = OpenClawCliProviderAdapter {
                cli_bin: openclaw_cli_bin.to_string(),
                source_provider: other.to_string(),
                env_scope: openclaw_env_scope.clone(),
                timeout_seconds: provider_timeout_seconds,
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

fn canonical_openclaw_provider(provider: &str) -> &str {
    match provider {
        "codex" => "openai-codex",
        "minimax-cn" => "minimax",
        other => other,
    }
}
