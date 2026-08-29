fn parse_action(raw: &str) -> Result<SettlementAction, WorkerError> {
    match raw.trim() {
        "consume" => Ok(SettlementAction::Consume),
        "refund" => Ok(SettlementAction::Refund),
        other => Err(WorkerError::Database(format!(
            "unsupported claimed settlement action '{other}'"
        ))),
    }
}

fn exponential_retry_seconds(attempt_count: i32) -> i32 {
    let exponent = attempt_count.saturating_sub(1).clamp(0, 10) as u32;
    2_i32.saturating_pow(exponent).clamp(1, 3_600)
}

fn validate_serial_lease_budget(
    batch_size: i64,
    request_timeout_seconds: u64,
    lease_seconds: i64,
) -> Result<(), WorkerError> {
    let batch = u64::try_from(batch_size)
        .map_err(|_| WorkerError::Config("batch size must be positive".to_string()))?;
    let lease = u64::try_from(lease_seconds)
        .map_err(|_| WorkerError::Config("lease seconds must be positive".to_string()))?;
    let required = DATABASE_OPERATION_TIMEOUT_SECONDS
        .saturating_add(
            batch.saturating_mul(
                request_timeout_seconds
                    .saturating_add(DATABASE_OPERATION_TIMEOUT_SECONDS.saturating_mul(2)),
            ),
        )
        .saturating_add(LEASE_SAFETY_MARGIN_SECONDS);
    if required >= lease {
        return Err(WorkerError::Config(format!(
            "serial settlement lease budget is unsafe: batch_size={batch_size}, request_timeout={request_timeout_seconds}s requires lease greater than {required}s, configured {lease_seconds}s"
        )));
    }
    Ok(())
}

fn validate_worker_id(worker_id: &str) -> Result<(), WorkerError> {
    if worker_id.is_empty()
        || worker_id.len() > 128
        || !worker_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':')
        })
    {
        return Err(WorkerError::Config(
            "settlement worker id must use 1..128 characters from [A-Za-z0-9._:-]"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_runtime_posture(
    ledger_manage_token: &str,
    mode: ExecutionLedgerMode,
) -> Result<(), WorkerError> {
    let profile = resolve_profile()?;
    if profile.is_production_like() {
        if !matches!(mode, ExecutionLedgerMode::RequireV2 | ExecutionLedgerMode::Dual) {
            return Err(WorkerError::Config(
                "production-like settlement worker requires dual or require_v2 mode".to_string(),
            ));
        }
        let lowered = ledger_manage_token.to_ascii_lowercase();
        for marker in [
            "local-dev",
            "change-me",
            "changeme",
            "replace-me",
            "replace_",
            "insecure-default",
        ] {
            if lowered.contains(marker) {
                return Err(WorkerError::Config(format!(
                    "production-like ledger credential contains forbidden marker '{marker}'"
                )));
            }
        }
        if ledger_manage_token.len() < 32 {
            return Err(WorkerError::Config(
                "production-like ledger credential must be at least 32 bytes".to_string(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerProfile {
    Test,
    Local,
    Dev,
    Beta,
    Staging,
    Production,
}

impl WorkerProfile {
    fn parse(raw: &str) -> Result<Self, WorkerError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "test" => Ok(Self::Test),
            "local" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "staging" | "stage" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            other => Err(WorkerError::Config(format!(
                "unsupported runtime profile '{other}'"
            ))),
        }
    }

    fn is_production_like(self) -> bool {
        matches!(self, Self::Beta | Self::Staging | Self::Production)
    }
}

fn resolve_profile() -> Result<WorkerProfile, WorkerError> {
    let primary = env::var("CEX_RUNTIME_PROFILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| WorkerProfile::parse(&value))
        .transpose()?;
    let compatibility = env::var("APP_ENV")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| WorkerProfile::parse(&value))
        .transpose()?;
    match (primary, compatibility) {
        (Some(left), Some(right)) if left != right => Err(WorkerError::Config(
            "CEX_RUNTIME_PROFILE and APP_ENV resolve to different profiles".to_string(),
        )),
        (Some(profile), _) | (_, Some(profile)) => Ok(profile),
        (None, None) => Err(WorkerError::Config(
            "CEX_RUNTIME_PROFILE or APP_ENV must be set explicitly".to_string(),
        )),
    }
}

fn required_env(name: &str) -> Result<String, WorkerError> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| WorkerError::Config(format!("{name} is required")))
}

fn bounded_i64_env(
    name: &str,
    default_value: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<i64>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bounded_u64_env(
    name: &str,
    default_value: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bounded_u32_env(
    name: &str,
    default_value: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, WorkerError> {
    let value = match env::var(name) {
        Ok(raw) => raw
            .trim()
            .parse::<u32>()
            .map_err(|_| WorkerError::Config(format!("{name} must be an integer")))?,
        Err(_) => default_value,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(WorkerError::Config(format!(
            "{name} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

fn bool_env(name: &str, default_value: bool) -> Result<bool, WorkerError> {
    match env::var(name) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(WorkerError::Config(format!(
                "{name} must be a boolean"
            ))),
        },
        Err(_) => Ok(default_value),
    }
}
