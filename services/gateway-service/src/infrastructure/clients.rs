use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use shared_config::{
    load_execution_scoped_admin_tokens, load_ledger_scoped_admin_tokens,
    select_execution_manage_token, select_execution_read_or_manage_token,
    select_ledger_manage_token, select_ledger_read_or_manage_token,
};
use shared_types::{
    ApiKeyResolveRequest, AuditEventCreateRequest, AuthContext, CapabilityRecord,
    ExecutionDispatchMode, ExecutionStatus,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerActionRequest {
    pub account_id: Uuid,
    pub amount: f64,
    pub reference_id: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCreateRequest {
    pub invocation_id: Uuid,
    pub trace_id: Uuid,
    pub org_id: Option<String>,
    pub actor_id: Option<String>,
    pub capability_id: Option<String>,
    pub capability_provider: Option<String>,
    pub capability_provider_ref: Option<String>,
    pub prompt: String,
    pub reserve_amount: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountLookupResult {
    pub account_id: Uuid,
    pub org_id: String,
    pub account_type: String,
    pub currency_unit: String,
    pub balance: f64,
    pub reserved: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCreateResult {
    pub execution_id: Uuid,
    pub status: ExecutionStatus,
    pub approval_required: bool,
    pub policy_reason: Option<String>,
    #[serde(default)]
    pub dispatch_mode: ExecutionDispatchMode,
    #[serde(default)]
    pub attempt_count: i32,
    #[serde(default)]
    pub max_attempts: i32,
}

impl ExecutionCreateResult {
    pub fn attempts_remaining(&self) -> i32 {
        (self.max_attempts - self.attempt_count).max(0)
    }

    pub fn retry_budget_exhausted(&self) -> bool {
        self.attempts_remaining() == 0
    }
}

#[derive(Debug, Clone)]
pub struct ServiceCallError {
    pub status: Option<u16>,
    pub message: String,
}

impl ServiceCallError {
    fn transport(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    fn upstream(status: u16, body: impl Into<String>) -> Self {
        let body = body.into();
        let detail = extract_upstream_error_detail(&body);
        Self {
            status: Some(status),
            message: if detail.trim().is_empty() {
                format!("upstream returned status {status}")
            } else {
                format!("upstream returned status {status}: {detail}")
            },
        }
    }

    fn decode(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }
}

fn extract_upstream_error_detail(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
            return message.to_string();
        }
        if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
            return error.to_string();
        }
    }

    trimmed.to_string()
}

#[derive(Debug, Clone)]
pub struct ServiceClients {
    pub identity_base_url: String,
    pub ledger_base_url: String,
    pub execution_base_url: String,
    pub audit_base_url: String,
    pub capability_base_url: String,
    pub ledger_manage_token: String,
    pub ledger_read_token: String,
    pub execution_manage_token: String,
    pub execution_read_token: String,
    pub http: reqwest::Client,
}

impl ServiceClients {
    pub fn new(
        identity_base_url: String,
        ledger_base_url: String,
        execution_base_url: String,
        audit_base_url: String,
        capability_base_url: String,
    ) -> Self {
        let ledger_tokens = load_ledger_scoped_admin_tokens();
        let ledger_manage_token = select_ledger_manage_token(&ledger_tokens)
            .expect("ledger manage token")
            .token
            .clone();
        let ledger_read_token = select_ledger_read_or_manage_token(&ledger_tokens)
            .expect("ledger read token")
            .token
            .clone();
        let execution_tokens = load_execution_scoped_admin_tokens();
        let execution_manage_token = select_execution_manage_token(&execution_tokens)
            .expect("execution manage token")
            .token
            .clone();
        let execution_read_token = select_execution_read_or_manage_token(&execution_tokens)
            .expect("execution read token")
            .token
            .clone();

        Self {
            identity_base_url,
            ledger_base_url,
            execution_base_url,
            audit_base_url,
            capability_base_url,
            ledger_manage_token,
            ledger_read_token,
            execution_manage_token,
            execution_read_token,
            http: reqwest::Client::new(),
        }
    }

    async fn post_expect_json<Req, Resp>(
        &self,
        url: String,
        req: &Req,
    ) -> Result<Resp, ServiceCallError>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
    {
        let resp = self
            .http
            .post(url)
            .json(req)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        serde_json::from_str(&body).map_err(|e| {
            ServiceCallError::decode(format!("decode response failed: {e}; body={body}"))
        })
    }

    async fn post_expect_success<Req>(&self, url: String, req: &Req) -> Result<(), ServiceCallError>
    where
        Req: Serialize + ?Sized,
    {
        let resp = self
            .http
            .post(url)
            .json(req)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        Ok(())
    }

    async fn post_expect_ledger_success<Req>(
        &self,
        url: String,
        req: &Req,
    ) -> Result<(), ServiceCallError>
    where
        Req: Serialize + ?Sized,
    {
        let resp = self
            .http
            .post(url)
            .header("x-admin-token", &self.ledger_manage_token)
            .json(req)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        Ok(())
    }

    async fn post_expect_execution_json<Req, Resp>(
        &self,
        url: String,
        req: &Req,
        admin_token: &str,
    ) -> Result<Resp, ServiceCallError>
    where
        Req: Serialize + ?Sized,
        Resp: DeserializeOwned,
    {
        let resp = self
            .http
            .post(url)
            .header("x-admin-token", admin_token)
            .json(req)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        serde_json::from_str(&body).map_err(|e| {
            ServiceCallError::decode(format!("decode response failed: {e}; body={body}"))
        })
    }

    pub async fn resolve_api_key(
        &self,
        req: &ApiKeyResolveRequest,
    ) -> Result<AuthContext, ServiceCallError> {
        let url = format!("{}/v1/auth/resolve", self.identity_base_url);
        self.post_expect_json(url, req).await
    }

    pub async fn get_account(
        &self,
        account_id: Uuid,
    ) -> Result<AccountLookupResult, ServiceCallError> {
        let url = format!("{}/v1/accounts/{}", self.ledger_base_url, account_id);
        let resp = self
            .http
            .get(url)
            .header("x-admin-token", &self.ledger_read_token)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        serde_json::from_str(&body).map_err(|e| {
            ServiceCallError::decode(format!("decode response failed: {e}; body={body}"))
        })
    }

    /// Legacy compatibility write. Canonical Invocation ingress rejects this
    /// path unless the explicitly governed break-glass switch is enabled.
    pub async fn reserve_credits_legacy_v1(
        &self,
        req: &LedgerActionRequest,
    ) -> Result<(), ServiceCallError> {
        let url = format!("{}/v1/ledger/reserve", self.ledger_base_url);
        self.post_expect_ledger_success(url, req).await
    }

    /// Legacy compatibility write. Canonical Invocation ingress rejects this
    /// path unless the explicitly governed break-glass switch is enabled.
    pub async fn refund_credits_legacy_v1(
        &self,
        req: &LedgerActionRequest,
    ) -> Result<(), ServiceCallError> {
        let url = format!("{}/v1/ledger/refund", self.ledger_base_url);
        self.post_expect_ledger_success(url, req).await
    }

    pub async fn create_execution(
        &self,
        req: &ExecutionCreateRequest,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!("{}/v1/executions", self.execution_base_url);
        self.post_expect_json(url, req).await
    }

    pub async fn get_execution(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!("{}/v1/executions/{}", self.execution_base_url, execution_id);
        let resp = self
            .http
            .get(url)
            .header("x-admin-token", &self.execution_read_token)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        serde_json::from_str(&body).map_err(|e| {
            ServiceCallError::decode(format!("decode response failed: {e}; body={body}"))
        })
    }

    pub async fn start_execution(
        &self,
        execution_id: Uuid,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!(
            "{}/v1/executions/{}/start",
            self.execution_base_url, execution_id
        );
        self.post_expect_execution_json(
            url,
            &serde_json::json!({
                "started_by": "gateway-service",
                "note": "auto-start provider execution"
            }),
            &self.execution_manage_token,
        )
        .await
    }

    pub async fn approve_execution(
        &self,
        execution_id: Uuid,
        approved_by: &str,
        note: Option<&str>,
        admin_token: &str,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!(
            "{}/v1/executions/{}/approve",
            self.execution_base_url, execution_id
        );
        self.post_expect_execution_json(
            url,
            &serde_json::json!({
                "approved_by": approved_by,
                "note": note,
            }),
            admin_token,
        )
        .await
    }

    pub async fn retry_execution(
        &self,
        execution_id: Uuid,
        retried_by: &str,
        note: Option<&str>,
        admin_token: &str,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!(
            "{}/v1/executions/{}/retry",
            self.execution_base_url, execution_id
        );
        self.post_expect_execution_json(
            url,
            &serde_json::json!({
                "retried_by": retried_by,
                "note": note,
            }),
            admin_token,
        )
        .await
    }

    pub async fn cancel_execution(
        &self,
        execution_id: Uuid,
        cancelled_by: &str,
        reason: &str,
        admin_token: &str,
    ) -> Result<ExecutionCreateResult, ServiceCallError> {
        let url = format!(
            "{}/v1/executions/{}/cancel",
            self.execution_base_url, execution_id
        );
        self.post_expect_execution_json(
            url,
            &serde_json::json!({
                "cancelled_by": cancelled_by,
                "reason": reason,
            }),
            admin_token,
        )
        .await
    }

    pub async fn get_capability(
        &self,
        capability_id: &str,
    ) -> Result<CapabilityRecord, ServiceCallError> {
        let url = format!(
            "{}/v1/capabilities/{}",
            self.capability_base_url, capability_id
        );
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| ServiceCallError::transport(format!("request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| ServiceCallError::transport(format!("read body failed: {e}")))?;

        if !status.is_success() {
            return Err(ServiceCallError::upstream(status.as_u16(), body));
        }

        serde_json::from_str(&body).map_err(|e| {
            ServiceCallError::decode(format!("decode response failed: {e}; body={body}"))
        })
    }

    pub async fn emit_audit_event(
        &self,
        trace_id: Uuid,
        org_id: Option<String>,
        actor_type: impl Into<String>,
        actor_id: Option<String>,
        event_type: impl Into<String>,
        payload: Value,
    ) -> Result<(), ServiceCallError> {
        let req = AuditEventCreateRequest {
            trace_id,
            org_id,
            actor_type: actor_type.into(),
            actor_id,
            event_type: event_type.into(),
            payload,
        };
        let url = format!("{}/v1/audit/events", self.audit_base_url);
        self.post_expect_success(url, &req).await
    }
}
