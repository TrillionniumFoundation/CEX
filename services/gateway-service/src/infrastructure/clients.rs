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

#[derive(Debug, Clone)]
pub struct AccountLookupResult {
    pub account_id: Uuid,
    pub org_id: String,
    pub account_type: String,
    pub currency_unit: String,
    pub currency_scale: u8,
    pub balance_minor: i64,
    pub reserved_minor: i64,
    pub available_minor: i64,
    /// Compatibility/display projection for callers that still consume the
    /// pre-v2 major-unit fields. Exact authorization logic must use *_minor.
    pub balance: f64,
    pub reserved: f64,
}

const MAX_EXACT_POWER10: u32 = 38;

#[derive(Debug, Deserialize)]
struct AccountLookupWire {
    account_id: Uuid,
    org_id: String,
    #[serde(default)]
    account_type: Option<String>,
    currency_unit: String,
    #[serde(default)]
    currency_scale: Option<Value>,
    #[serde(default)]
    balance_minor: Option<Value>,
    #[serde(default)]
    reserved_minor: Option<Value>,
    #[serde(default)]
    balance: Option<Value>,
    #[serde(default)]
    reserved: Option<Value>,
}

fn account_value_text(value: &Value, field: &str) -> Result<String, String> {
    match value {
        Value::String(raw) => Ok(raw.trim().to_string()),
        Value::Number(number) => Ok(number.to_string()),
        _ => Err(format!("{field} must be a decimal number or string")),
    }
}

fn account_currency_scale(value: Option<&Value>, required: bool) -> Result<u8, String> {
    let Some(value) = value else {
        return if required {
            Err("currency_scale is required for exact account responses".to_string())
        } else {
            // Historical v1 ledger responses represented credits at six
            // decimal places when currency_scale was omitted.
            Ok(6)
        };
    };
    let raw = account_value_text(value, "currency_scale")?;
    let parsed = raw
        .parse::<u16>()
        .map_err(|_| "currency_scale must be an integer".to_string())?;
    u8::try_from(parsed)
        .ok()
        .filter(|scale| *scale <= 6)
        .ok_or_else(|| "currency_scale must be between 0 and 6".to_string())
}

fn account_minor_integer(value: &Value, field: &str) -> Result<i64, String> {
    let raw = account_value_text(value, field)?;
    if raw.is_empty()
        || raw.starts_with('-')
        || raw.starts_with('+')
        || !raw.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("{field} must be a non-negative integer"));
    }
    let parsed = raw
        .parse::<u128>()
        .map_err(|_| format!("{field} is outside the supported integer range"))?;
    i64::try_from(parsed).map_err(|_| format!("{field} is outside the supported integer range"))
}

fn account_checked_pow10(power: u32) -> Option<i128> {
    // i64 money values can never need more than 38 decimal places of scaling;
    // cap the loop before handling untrusted scientific-notation exponents.
    if power > MAX_EXACT_POWER10 {
        return None;
    }
    let mut result = 1_i128;
    for _ in 0..power {
        result = result.checked_mul(10)?;
    }
    Some(result)
}

/// Convert a legacy major-unit value to minor units exactly. This parser is
/// intentionally decimal/integer based; it never performs money arithmetic
/// using f64. The resulting f64 fields are only a compatibility projection.
fn account_legacy_major_to_minor(
    value: &Value,
    currency_scale: u8,
    field: &str,
) -> Result<i64, String> {
    let raw = account_value_text(value, field)?;
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('-') || raw.starts_with('+') {
        return Err(format!("{field} must be a non-negative decimal"));
    }
    let (mantissa, exponent) = match raw.find(['e', 'E']) {
        Some(index) => (
            &raw[..index],
            raw[index + 1..]
                .parse::<i64>()
                .map_err(|_| format!("{field} has an invalid exponent"))?,
        ),
        None => (raw, 0),
    };
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, fraction),
        None => (mantissa, ""),
    };
    if (integer.is_empty() && fraction.is_empty())
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("{field} has an invalid decimal"));
    }
    let coefficient = format!("{integer}{fraction}")
        .parse::<i128>()
        .map_err(|_| format!("{field} is outside the supported integer range"))?;
    let shift = exponent
        .checked_sub(i64::try_from(fraction.len()).map_err(|_| format!("{field} is too long"))?)
        .and_then(|value| value.checked_add(i64::from(currency_scale)))
        .ok_or_else(|| format!("{field} exponent is outside the supported range"))?;
    let scaled = if shift >= 0 {
        let power = u32::try_from(shift).map_err(|_| format!("{field} is too large"))?;
        coefficient
            .checked_mul(
                account_checked_pow10(power).ok_or_else(|| format!("{field} is too large"))?,
            )
            .ok_or_else(|| format!("{field} is outside the supported integer range"))?
    } else {
        let magnitude = shift
            .checked_neg()
            .ok_or_else(|| format!("{field} exponent is outside the supported range"))?;
        let power = u32::try_from(magnitude).map_err(|_| format!("{field} is too small"))?;
        let divisor =
            account_checked_pow10(power).ok_or_else(|| format!("{field} is too small"))?;
        if coefficient % divisor != 0 {
            return Err(format!("{field} has more precision than currency_scale"));
        }
        coefficient / divisor
    };
    i64::try_from(scaled).map_err(|_| format!("{field} is outside the supported integer range"))
}

fn account_minor_to_legacy_display(minor: i64, currency_scale: u8) -> Result<f64, String> {
    // This conversion is deliberately confined to the legacy response
    // boundary. All comparisons and arithmetic use the integer fields above.
    let factor = 10_f64.powi(i32::from(currency_scale));
    let value = (minor as f64) / factor;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("minor amount cannot be represented for legacy display".to_string())
    }
}

fn decode_account_lookup_wire(wire: AccountLookupWire) -> Result<AccountLookupResult, String> {
    let (balance_minor, reserved_minor, currency_scale) =
        match (wire.balance_minor.as_ref(), wire.reserved_minor.as_ref()) {
            (Some(balance), Some(reserved)) => (
                account_minor_integer(balance, "balance_minor")?,
                account_minor_integer(reserved, "reserved_minor")?,
                account_currency_scale(wire.currency_scale.as_ref(), true)?,
            ),
            (None, None) => {
                let currency_scale = account_currency_scale(wire.currency_scale.as_ref(), false)?;
                (
                    account_legacy_major_to_minor(
                        wire.balance
                            .as_ref()
                            .ok_or_else(|| "balance is missing".to_string())?,
                        currency_scale,
                        "balance",
                    )?,
                    account_legacy_major_to_minor(
                        wire.reserved
                            .as_ref()
                            .ok_or_else(|| "reserved is missing".to_string())?,
                        currency_scale,
                        "reserved",
                    )?,
                    currency_scale,
                )
            }
            _ => {
                return Err("balance_minor and reserved_minor must be provided together".to_string())
            }
        };
    let available_minor = balance_minor
        .checked_sub(reserved_minor)
        .ok_or_else(|| "reserved_minor exceeds balance_minor".to_string())?;
    if available_minor < 0 {
        return Err("reserved_minor exceeds balance_minor".to_string());
    }

    Ok(AccountLookupResult {
        account_id: wire.account_id,
        org_id: wire.org_id,
        account_type: wire.account_type.unwrap_or_default(),
        currency_unit: wire.currency_unit,
        currency_scale,
        balance_minor,
        reserved_minor,
        available_minor,
        balance: account_minor_to_legacy_display(balance_minor, currency_scale)?,
        reserved: account_minor_to_legacy_display(reserved_minor, currency_scale)?,
    })
}

impl<'de> Deserialize<'de> for AccountLookupResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = AccountLookupWire::deserialize(deserializer)?;
        decode_account_lookup_wire(wire).map_err(serde::de::Error::custom)
    }
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

#[cfg(test)]
mod account_lookup_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_ledger_account_response_decodes_without_legacy_fields() {
        let account: AccountLookupResult = serde_json::from_value(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "currency_scale": 6,
            "balance_minor": "123456789",
            "reserved_minor": "456789",
            "schema_version": "ledger.account.v2"
        }))
        .expect("exact ledger response should decode");

        assert_eq!(account.account_type, "");
        assert_eq!(account.currency_scale, 6);
        assert_eq!(account.balance_minor, 123_456_789);
        assert_eq!(account.reserved_minor, 456_789);
        assert_eq!(account.available_minor, 123_000_000);
        assert!((account.balance - 123.456789).abs() < 1e-9);
    }

    #[test]
    fn legacy_account_response_remains_compatible() {
        let account: AccountLookupResult = serde_json::from_value(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "account_type": "org_wallet",
            "currency_unit": "credit",
            "currency_scale": 6,
            "balance": 100.25,
            "reserved": "0.125000"
        }))
        .expect("legacy ledger response should decode");

        assert_eq!(account.account_type, "org_wallet");
        assert_eq!(account.balance_minor, 100_250_000);
        assert_eq!(account.reserved_minor, 125_000);
        assert_eq!(account.available_minor, 100_125_000);
    }

    #[test]
    fn account_lookup_rejects_precision_loss_and_negative_available() {
        let precision_error = serde_json::from_value::<AccountLookupResult>(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "currency_scale": 6,
            "balance": "1.0000001",
            "reserved": "0"
        }))
        .expect_err("precision loss must be rejected");
        assert!(precision_error.to_string().contains("more precision"));

        let negative_error = serde_json::from_value::<AccountLookupResult>(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "currency_scale": 6,
            "balance_minor": "1",
            "reserved_minor": "2"
        }))
        .expect_err("reserved amount cannot exceed balance");
        assert!(negative_error.to_string().contains("exceeds"));

        let extreme_exponent = serde_json::from_value::<AccountLookupResult>(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "balance": "1e-9223372036854775808",
            "reserved": "0"
        }))
        .expect_err("extreme exponent must fail closed");
        assert!(!extreme_exponent.to_string().is_empty());

        let huge_positive_exponent = serde_json::from_value::<AccountLookupResult>(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "balance": "1e1000000000",
            "reserved": "0"
        }))
        .expect_err("huge positive exponent must fail closed without an unbounded loop");
        assert!(!huge_positive_exponent.to_string().is_empty());

        let unsupported_scale = serde_json::from_value::<AccountLookupResult>(json!({
            "account_id": "00000000-0000-0000-0000-000000000001",
            "org_id": "org-1",
            "currency_unit": "credit",
            "currency_scale": 7,
            "balance_minor": "1",
            "reserved_minor": "0"
        }))
        .expect_err("Ledger supports only scales through six");
        assert!(unsupported_scale.to_string().contains("between 0 and 6"));
    }
}
