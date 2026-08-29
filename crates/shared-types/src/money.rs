use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const DEFAULT_CREDIT_SCALE: u8 = 6;
pub const MAX_MONEY_SCALE: u8 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoneyAmount {
    pub currency: String,
    pub scale: u8,
    #[serde(with = "i64_string")]
    pub minor_units: i64,
}

impl MoneyAmount {
    pub fn new(
        currency: impl Into<String>,
        scale: u8,
        minor_units: i64,
    ) -> Result<Self, MoneyError> {
        Ok(Self {
            currency: normalize_currency(currency.into())?,
            scale: validate_scale(scale)?,
            minor_units,
        })
    }

    pub fn credits(minor_units: i64) -> Self {
        Self {
            currency: "credit".to_string(),
            scale: DEFAULT_CREDIT_SCALE,
            minor_units,
        }
    }

    pub fn parse_decimal(
        currency: impl Into<String>,
        scale: u8,
        raw: &str,
    ) -> Result<Self, MoneyError> {
        let scale = validate_scale(scale)?;
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(MoneyError::InvalidDecimal("amount is empty".to_string()));
        }
        if raw.contains('e') || raw.contains('E') {
            return Err(MoneyError::InvalidDecimal(
                "scientific notation is forbidden".to_string(),
            ));
        }

        let (negative, unsigned) = match raw.as_bytes().first() {
            Some(b'-') => (true, &raw[1..]),
            Some(b'+') => (false, &raw[1..]),
            _ => (false, raw),
        };
        if unsigned.is_empty() {
            return Err(MoneyError::InvalidDecimal(
                "amount has no digits".to_string(),
            ));
        }

        let mut parts = unsigned.split('.');
        let whole = parts.next().unwrap_or_default();
        let fraction = parts.next().unwrap_or_default();
        if parts.next().is_some() {
            return Err(MoneyError::InvalidDecimal(
                "amount contains multiple decimal points".to_string(),
            ));
        }
        if (whole.is_empty() && fraction.is_empty())
            || !whole.chars().all(|ch| ch.is_ascii_digit())
            || !fraction.chars().all(|ch| ch.is_ascii_digit())
        {
            return Err(MoneyError::InvalidDecimal(
                "amount contains invalid decimal digits".to_string(),
            ));
        }
        if fraction.len() > usize::from(scale) {
            return Err(MoneyError::ScaleExceeded {
                configured: scale,
                actual: fraction.len(),
            });
        }

        let whole = if whole.is_empty() {
            0_i128
        } else {
            whole.parse::<i128>().map_err(|_| MoneyError::Overflow)?
        };
        let factor = 10_i128.pow(u32::from(scale));
        let mut total = whole.checked_mul(factor).ok_or(MoneyError::Overflow)?;

        if !fraction.is_empty() {
            let fraction_value = fraction.parse::<i128>().map_err(|_| MoneyError::Overflow)?;
            let padding = u32::from(scale)
                - u32::try_from(fraction.len()).map_err(|_| MoneyError::Overflow)?;
            let fraction_minor = fraction_value
                .checked_mul(10_i128.pow(padding))
                .ok_or(MoneyError::Overflow)?;
            total = total
                .checked_add(fraction_minor)
                .ok_or(MoneyError::Overflow)?;
        }

        if negative {
            total = total.checked_neg().ok_or(MoneyError::Overflow)?;
        }
        let minor_units = i64::try_from(total).map_err(|_| MoneyError::Overflow)?;
        Self::new(currency, scale, minor_units)
    }

    pub fn format_decimal(&self) -> String {
        let factor = 10_i128.pow(u32::from(self.scale));
        let value = i128::from(self.minor_units);
        let absolute = value.abs();
        let sign = if value.is_negative() { "-" } else { "" };
        let whole = absolute / factor;

        if self.scale == 0 {
            return format!("{sign}{whole}");
        }

        let fraction = absolute % factor;
        format!(
            "{sign}{whole}.{:0width$}",
            fraction,
            width = usize::from(self.scale)
        )
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, MoneyError> {
        self.require_compatible(other)?;
        Self::new(
            self.currency.clone(),
            self.scale,
            self.minor_units
                .checked_add(other.minor_units)
                .ok_or(MoneyError::Overflow)?,
        )
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, MoneyError> {
        self.require_compatible(other)?;
        Self::new(
            self.currency.clone(),
            self.scale,
            self.minor_units
                .checked_sub(other.minor_units)
                .ok_or(MoneyError::Overflow)?,
        )
    }

    pub fn require_positive(&self) -> Result<(), MoneyError> {
        if self.minor_units > 0 {
            Ok(())
        } else {
            Err(MoneyError::NonPositive)
        }
    }

    fn require_compatible(&self, other: &Self) -> Result<(), MoneyError> {
        if self.currency == other.currency && self.scale == other.scale {
            Ok(())
        } else {
            Err(MoneyError::Incompatible {
                left: format!("{}/{}", self.currency, self.scale),
                right: format!("{}/{}", other.currency, other.scale),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoneyError {
    InvalidCurrency(String),
    InvalidScale(u8),
    InvalidDecimal(String),
    ScaleExceeded { configured: u8, actual: usize },
    Incompatible { left: String, right: String },
    NonPositive,
    Overflow,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrency(value) => write!(formatter, "invalid currency '{value}'"),
            Self::InvalidScale(value) => write!(
                formatter,
                "invalid money scale {value}; maximum is {MAX_MONEY_SCALE}"
            ),
            Self::InvalidDecimal(message) => write!(formatter, "invalid decimal amount: {message}"),
            Self::ScaleExceeded { configured, actual } => write!(
                formatter,
                "amount has {actual} fractional digits; configured scale is {configured}"
            ),
            Self::Incompatible { left, right } => {
                write!(formatter, "incompatible money values: {left} vs {right}")
            }
            Self::NonPositive => formatter.write_str("money amount must be positive"),
            Self::Overflow => formatter.write_str("money amount exceeds i64 minor-unit range"),
        }
    }
}

impl Error for MoneyError {}

fn normalize_currency(currency: String) -> Result<String, MoneyError> {
    let currency = currency.trim().to_ascii_lowercase();
    if currency.is_empty()
        || currency.len() > 16
        || !currency.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        })
    {
        return Err(MoneyError::InvalidCurrency(currency));
    }
    Ok(currency)
}

fn validate_scale(scale: u8) -> Result<u8, MoneyError> {
    if scale <= MAX_MONEY_SCALE {
        Ok(scale)
    } else {
        Err(MoneyError::InvalidScale(scale))
    }
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

    #[test]
    fn parser_is_exact_and_rejects_implicit_rounding() {
        let amount = MoneyAmount::parse_decimal("credit", 6, "12.34").unwrap();
        assert_eq!(amount.minor_units, 12_340_000);
        assert_eq!(amount.format_decimal(), "12.340000");
        assert!(matches!(
            MoneyAmount::parse_decimal("credit", 6, "1.0000001"),
            Err(MoneyError::ScaleExceeded { .. })
        ));
        assert!(MoneyAmount::parse_decimal("credit", 6, "1e3").is_err());
    }

    #[test]
    fn parser_handles_negative_minimum_unit() {
        let amount = MoneyAmount::parse_decimal("credit", 6, "-0.000001").unwrap();
        assert_eq!(amount.minor_units, -1);
        assert_eq!(amount.format_decimal(), "-0.000001");
    }

    #[test]
    fn arithmetic_requires_same_currency_and_scale() {
        let left = MoneyAmount::credits(10);
        let other = MoneyAmount::new("wallet_credit", 6, 5).unwrap();
        assert!(matches!(
            left.checked_add(&other),
            Err(MoneyError::Incompatible { .. })
        ));
    }

    #[test]
    fn json_uses_string_for_large_minor_units() {
        let amount = MoneyAmount::credits(9_007_199_254_740_993);
        let value = serde_json::to_value(&amount).unwrap();
        assert_eq!(value["minor_units"], "9007199254740993");
        assert_eq!(
            serde_json::from_value::<MoneyAmount>(value).unwrap(),
            amount
        );
    }

    #[test]
    fn checked_add_detects_overflow() {
        assert!(matches!(
            MoneyAmount::credits(i64::MAX).checked_add(&MoneyAmount::credits(1)),
            Err(MoneyError::Overflow)
        ));
    }
}
