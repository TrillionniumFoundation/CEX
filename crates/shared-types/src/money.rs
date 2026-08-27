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
        let currency = normalize_currency(currency.into())?;
        validate_scale(scale)?;
        Ok(Self {
            currency,
            scale,
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
        validate_scale(scale)?;
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(MoneyError::InvalidDecimal("amount is empty".to_string()));
        }
        if raw.contains(['e', 'E']) {
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
                "amount contains more than one decimal point".to_string(),
            ));
        }
        if whole.is_empty() && fraction.is_empty() {
            return Err(MoneyError::InvalidDecimal(
                "amount has no digits".to_string(),
            ));
        }
        if !whole.chars().all(|ch| ch.is_ascii_digit())
            || !fraction.chars().all(|ch| ch.is_ascii_digit())
        {
            return Err(MoneyError::InvalidDecimal(
                "amount contains non-decimal digits".to_string(),
            ));
        }
        if fraction.len() > usize::from(scale) {
            return Err(MoneyError::ScaleExceeded {
                configured: scale,
                actual: fraction.len(),
            });
        }

        let whole_value = if whole.is_empty() {
            0_i64
        } else {
            whole
                .parse::<i64>()
                .map_err(|_| MoneyError::Overflow)?
        };
        let factor = scale_factor(scale)?;
        let mut minor_units = whole_value
            .checked_mul(factor)
            .ok_or(MoneyError::Overflow)?;

        if !fraction.is_empty() {
            let fraction_value = fraction
                .parse::<i64>()
                .map_err(|_| MoneyError::Overflow)?;
            let padding = u32::from(scale)
                .checked_sub(
                    u32::try_from(fraction.len()).map_err(|_| MoneyError::Overflow)?,
                )
                .ok_or(MoneyError::Overflow)?;
            let padded = fraction_value
                .checked_mul(10_i64.checked_pow(padding).ok_or(MoneyError::Overflow)?)
                .ok_or(MoneyError::Overflow)?;
            minor_units = minor_units
                .checked_add(padded)
                .ok_or(MoneyError::Overflow)?;
        }

        if negative {
            minor_units = minor_units.checked_neg().ok_or(MoneyError::Overflow)?;
        }

        Self::new(currency, scale, minor_units)
    }

    pub fn format_decimal(&self) -> String {
        let factor = scale_factor(self.scale).expect("validated MoneyAmount scale");
        let negative = self.minor_units.is_negative();
        let absolute = i128::from(self.minor_units).abs();
        let factor = i128::from(factor);
        let whole = absolute / factor;
        let fraction = absolute % factor;

        if self.scale == 0 {
            return format!("{}{}", if negative { "-" } else { "" }, whole);
        }

        format!(
            "{}{}.{:0width$}",
            if negative { "-" } else { "" },
            whole,
            fraction,
            width = usize::from(self.scale),
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
                left_currency: self.currency.clone(),
                left_scale: self.scale,
                right_currency: other.currency.clone(),
                right_scale: other.scale,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoneyError {
    InvalidCurrency(String),
    InvalidScale(u8),
    InvalidDecimal(String),
    ScaleExceeded {
        configured: u8,
        actual: usize,
    },
    Incompatible {
        left_currency: String,
        left_scale: u8,
        right_currency: String,
        right_scale: u8,
    },
    NonPositive,
    Overflow,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCurrency(currency) => write!(
                formatter,
                "invalid currency '{currency}'; use 1-16 lowercase ASCII letters, digits, '.', '_' or '-'"
            ),
            Self::InvalidScale(scale) => write!(
                formatter,
                "invalid money scale {scale}; maximum supported scale is {MAX_MONEY_SCALE}"
            ),
            Self::InvalidDecimal(message) => write!(formatter, "invalid decimal amount: {message}"),
            Self::ScaleExceeded { configured, actual } => write!(
                formatter,
                "decimal amount has {actual} fractional digits, configured scale is {configured}"
            ),
            Self::Incompatible {
                left_currency,
                left_scale,
                right_currency,
                right_scale,
            } => write!(
                formatter,
                "incompatible money values: {left_currency}/{left_scale} vs {right_currency}/{right_scale}"
            ),
            Self::NonPositive => formatter.write_str("money amount must be positive"),
            Self::Overflow => formatter.write_str("money amount exceeds signed 64-bit minor-unit range"),
        }
    }
}

impl Error for MoneyError {}

fn normalize_currency(currency: String) -> Result<String, MoneyError> {
    let currency = currency.trim().to_ascii_lowercase();
    if currency.is_empty()
        || currency.len() > 16
        || !currency
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-'))
    {
        return Err(MoneyError::InvalidCurrency(currency));
    }
    Ok(currency)
}

fn validate_scale(scale: u8) -> Result<(), MoneyError> {
    if scale <= MAX_MONEY_SCALE {
        Ok(())
    } else {
        Err(MoneyError::InvalidScale(scale))
    }
}

fn scale_factor(scale: u8) -> Result<i64, MoneyError> {
    validate_scale(scale)?;
    10_i64
        .checked_pow(u32::from(scale))
        .ok_or(MoneyError::Overflow)
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
        let raw = String::deserialize(deserializer)?;
        raw.parse::<i64>().map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_parser_is_exact_and_pads_fraction() {
        let amount = MoneyAmount::parse_decimal("credit", 6, "12.34").unwrap();
        assert_eq!(amount.minor_units, 12_340_000);
        assert_eq!(amount.format_decimal(), "12.340000");
    }

    #[test]
    fn parser_handles_negative_values_without_float_conversion() {
        let amount = MoneyAmount::parse_decimal("credit", 6, "-0.000001").unwrap();
        assert_eq!(amount.minor_units, -1);
        assert_eq!(amount.format_decimal(), "-0.000001");
    }

    #[test]
    fn parser_rejects_excess_precision_and_scientific_notation() {
        assert!(matches!(
            MoneyAmount::parse_decimal("credit", 6, "1.0000001"),
            Err(MoneyError::ScaleExceeded { .. })
        ));
        assert!(MoneyAmount::parse_decimal("credit", 6, "1e3").is_err());
    }

    #[test]
    fn arithmetic_requires_matching_currency_and_scale() {
        let left = MoneyAmount::credits(10);
        let right = MoneyAmount::new("wallet_credit", 6, 5).unwrap();
        assert!(matches!(
            left.checked_add(&right),
            Err(MoneyError::Incompatible { .. })
        ));
    }

    #[test]
    fn minor_units_serialize_as_string_for_json_safety() {
        let amount = MoneyAmount::credits(9_007_199_254_740_993);
        let json = serde_json::to_value(&amount).unwrap();
        assert_eq!(json["minor_units"], "9007199254740993");
        let decoded: MoneyAmount = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, amount);
    }

    #[test]
    fn checked_arithmetic_detects_overflow() {
        let max = MoneyAmount::credits(i64::MAX);
        assert!(matches!(
            max.checked_add(&MoneyAmount::credits(1)),
            Err(MoneyError::Overflow)
        ));
    }
}
