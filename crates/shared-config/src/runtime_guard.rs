//! Stable public adapter around the reviewed startup-guard implementation.
//!
//! The implementation remains an exact Git blob in `runtime_guard_impl.rs`.
//! This boundary owns the post-validation normalization required by binaries
//! whose legacy state constructors still parse fail-fast flags locally.

mod implementation {
    include!("runtime_guard_impl.rs");
}

pub use implementation::{
    env_flag, parse_bool_value, resolve_runtime_profile, RuntimeProfile, ServiceKind, StartupError,
    StartupReport, CONFIG_ERROR_EXIT_CODE,
};

use std::env;

fn fail_fast_env(service: ServiceKind) -> &'static str {
    match service {
        ServiceKind::Gateway => "GATEWAY_FAIL_FAST",
        ServiceKind::Identity => "IDENTITY_FAIL_FAST",
        ServiceKind::Ledger => "LEDGER_FAIL_FAST",
        ServiceKind::Execution => "EXECUTION_FAIL_FAST",
        ServiceKind::Audit => "AUDIT_FAIL_FAST",
    }
}

/// Enforce the reviewed startup guard, then preserve its validated fail-fast
/// decision for all post-guard state constructors.
///
/// Production-like validation already requires the service flag to resolve to
/// true using trimmed, case-insensitive parsing. Canonicalizing that accepted
/// value to the exact string `true` prevents an older local parser from
/// reinterpreting values such as `ON` or ` true ` as false between database
/// preflight and state construction. The mutation occurs before worker threads
/// are started and never exposes a credential.
pub async fn enforce(service: ServiceKind) -> Result<StartupReport, StartupError> {
    let report = implementation::enforce(service).await?;
    if report.profile.is_production_like() {
        // SAFETY: startup enforcement runs before service worker threads are
        // created. The target is a non-secret boolean that was already
        // validated as true by the implementation above.
        unsafe {
            env::set_var(fail_fast_env(service), "true");
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_service_has_one_canonical_fail_fast_name() {
        assert_eq!(fail_fast_env(ServiceKind::Gateway), "GATEWAY_FAIL_FAST");
        assert_eq!(fail_fast_env(ServiceKind::Identity), "IDENTITY_FAIL_FAST");
        assert_eq!(fail_fast_env(ServiceKind::Ledger), "LEDGER_FAIL_FAST");
        assert_eq!(fail_fast_env(ServiceKind::Execution), "EXECUTION_FAIL_FAST");
        assert_eq!(fail_fast_env(ServiceKind::Audit), "AUDIT_FAIL_FAST");
    }
}
