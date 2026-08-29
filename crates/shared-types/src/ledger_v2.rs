use crate::money::{MoneyAmount, MoneyError, MAX_MONEY_SCALE};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use uuid::Uuid;

pub const LEDGER_EFFECT_SCHEMA_V1: &str = "cex.ledger.effect.v1";
pub const MAX_LEDGER_SCOPE_LEN: usize = 160;
pub const MAX_LEDGER_KEY_LEN: usize = 256;
pub const MAX_LEDGER_REFERENCE_TYPE_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerOperationKind {
    Reserve,
    Consume,
    Refund,
    Grant,
}

impl LedgerOperationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reserve => "reserve",
            Self::Consume => "consume",
            Self::Refund => "refund",
            Self::Grant => "grant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEffectRequestV1 {
    pub account_id: Uuid,
    pub trace_id: Option<Uuid>,
    pub operation_id: Option<Uuid>,
    pub operation_kind: LedgerOperationKind,
    pub currency_unit: String,
    pub currency_scale: u8,
    #[serde(with = "i64_string")]
    pub amount_minor: i64,
    pub reference_type: Option<String>,
    pub reference_id: Option<Uuid>,
    pub idempotency_scope: String,
    pub idempotency_key: String,
}

impl LedgerEffectRequestV1 {
    pub fn money(&self) -> Result<MoneyAmount, LedgerContractError> {
        MoneyAmount::new(
            self.currency_unit.clone(),
            self.currency_scale,
            self.amount_minor,
        )
        .map_err(LedgerContractError::Money)
    }

    pub fn validate(&self, require_explicit_trace: bool) -> Result<(), LedgerContractError> {
        if self.account_id.is_nil() {
            return Err(LedgerContractError::NilIdentifier("account_id"));
        }
        if self.trace_id.is_some_and(|value| value.is_nil()) {
            return Err(LedgerContractError::NilIdentifier("trace_id"));
        }
        if self.operation_id.is_some_and(|value| value.is_nil()) {
            return Err(LedgerContractError::NilIdentifier("operation_id"));
        }
        if require_explicit_trace && self.trace_id.is_none() {
            return Err(LedgerContractError::ExplicitTraceRequired);
        }

        let money = self.money()?;
        money
            .require_positive()
            .map_err(LedgerContractError::Money)?;
        if self.currency_scale > MAX_MONEY_SCALE {
            return Err(LedgerContractError::Money(MoneyError::InvalidScale(
                self.currency_scale,
            )));
        }

        validate_component(
            "idempotency_scope",
            &self.idempotency_scope,
            MAX_LEDGER_SCOPE_LEN,
        )?;

        let key = self.idempotency_key.trim();
        if key.is_empty()
            || key.chars().count() > MAX_LEDGER_KEY_LEN
            || key.chars().any(char::is_control)
        {
            return Err(LedgerContractError::InvalidIdempotencyKey);
        }

        if self.reference_type.is_some() != self.reference_id.is_some() {
            return Err(LedgerContractError::IncompleteReference);
        }
        if let Some(reference_type) = self.reference_type.as_deref() {
            validate_component(
                "reference_type",
                reference_type,
                MAX_LEDGER_REFERENCE_TYPE_LEN,
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerContractError {
    NilIdentifier(&'static str),
    ExplicitTraceRequired,
    Money(MoneyError),
    InvalidComponent {
        field: &'static str,
        maximum: usize,
    },
    InvalidIdempotencyKey,
    IncompleteReference,
}

impl LedgerContractError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NilIdentifier("account_id") => "invalid_account_id",
            Self::NilIdentifier("trace_id") => "invalid_trace_id",
            Self::NilIdentifier("operation_id") => "invalid_operation_id",
            Self::NilIdentifier(_) => "invalid_identifier",
            Self::ExplicitTraceRequired => "explicit_trace_required",
            Self::Money(MoneyError::NonPositive) => "invalid_amount_minor",
            Self::Money(MoneyError::InvalidCurrency(_)) => "invalid_currency_unit",
            Self::Money(MoneyError::InvalidScale(_)) => "invalid_currency_scale",
            Self::Money(_) => "invalid_money",
            Self::InvalidComponent {
                field: "idempotency_scope",
                ..
            } => "invalid_idempotency_scope",
            Self::InvalidComponent {
                field: "reference_type",
                ..
            } => "invalid_reference_type",
            Self::InvalidComponent { .. } => "invalid_ledger_component",
            Self::InvalidIdempotencyKey => "invalid_idempotency_key",
            Self::IncompleteReference => "invalid_reference_binding",
        }
    }
}

impl fmt::Display for LedgerContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NilIdentifier(field) => write!(formatter, "{field} must not be the nil UUID"),
            Self::ExplicitTraceRequired => {
                formatter.write_str("trace_id is required by the production ledger profile")
            }
            Self::Money(error) => write!(formatter, "invalid ledger money: {error}"),
            Self::InvalidComponent { field, maximum } => write!(
                formatter,
                "{field} must use 1..{maximum} characters from [A-Za-z0-9._:-]"
            ),
            Self::InvalidIdempotencyKey => formatter.write_str(
                "idempotency_key must contain 1..256 non-control characters",
            ),
            Self::IncompleteReference => formatter.write_str(
                "reference_type and reference_id must be supplied together",
            ),
        }
    }
}

impl Error for LedgerContractError {}

fn validate_component(
    field: &'static str,
    raw: &str,
    maximum: usize,
) -> Result<(), LedgerContractError> {
    let value = raw.trim();
    if value.is_empty()
        || value.len() > maximum
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err(LedgerContractError::InvalidComponent { field, maximum });
    }
    Ok(())
}

mod i64_string {
    use serde::{de::Error as _, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse::<i64>()
            .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> LedgerEffectRequestV1 {
        LedgerEffectRequestV1 {
            account_id: Uuid::new_v4(),
            trace_id: Some(Uuid::new_v4()),
            operation_id: None,
            operation_kind: LedgerOperationKind::Reserve,
            currency_unit: "credit".to_string(),
            currency_scale: 6,
            amount_minor: 1_250_000,
            reference_type: Some("invocation".to_string()),
            reference_id: Some(Uuid::new_v4()),
            idempotency_scope: "org:example:reserve".to_string(),
            idempotency_key: "reserve:operation".to_string(),
        }
    }

    #[test]
    fn exact_request_validates_without_floating_point() {
        let request = request();
        assert!(request.validate(true).is_ok());
        assert_eq!(request.money().unwrap().format_decimal(), "1.250000");
    }

    #[test]
    fn json_encodes_minor_units_as_a_string() {
        let value = serde_json::to_value(request()).unwrap();
        assert_eq!(value["amount_minor"], json!("1250000"));
    }

    #[test]
    fn incomplete_reference_and_missing_production_trace_are_rejected() {
        let mut request = request();
        request.reference_id = None;
        assert!(matches!(
            request.validate(true),
            Err(LedgerContractError::IncompleteReference)
        ));

        let mut request = self::request();
        request.trace_id = None;
        assert!(matches!(
            request.validate(true),
            Err(LedgerContractError::ExplicitTraceRequired)
        ));
    }

    #[test]
    fn scope_and_key_are_bounded() {
        let mut request = request();
        request.idempotency_scope = "bad scope".to_string();
        assert_eq!(
            request.validate(false).unwrap_err().code(),
            "invalid_idempotency_scope"
        );

        let mut request = self::request();
        request.idempotency_key = "\n".to_string();
        assert_eq!(
            request.validate(false).unwrap_err().code(),
            "invalid_idempotency_key"
        );
    }
}
