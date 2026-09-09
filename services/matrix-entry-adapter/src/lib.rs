#![recursion_limit = "256"]
#![forbid(unsafe_code)]

use axum::{middleware, Router};
use shared_config::runtime_guard::matrix_profile::resolve_profiles;

// The implementation is the byte-identical former crate root. It is private so
// callers cannot reach its legacy environment/config constructors. Moving a
// crate-level recursion attribute into a module is intentionally ignored; the
// active limit is declared above at this crate root. Its legacy public surface
// is retained only for its own regression tests and this facade.
#[allow(dead_code, unused_attributes)]
#[path = "implementation.rs"]
mod implementation;
mod delivery_binding;
mod reconciliation_response_binding;
mod result_reconciliation;

const PROFILE_ENV_NAMES: [&str; 3] = [
    "MATRIX_ENTRY_RUNTIME_PROFILE",
    "CEX_RUNTIME_PROFILE",
    "APP_ENV",
];

/// Proof that every declared Matrix profile source was read and accepted before
/// constructing the adapter state. The private field prevents caller invention.
#[must_use = "the validated environment must be consumed by AppState::from_validated_env"]
pub struct ValidatedMatrixAdapterEnvironment {
    _private: (),
}

/// Validate all supported profile sources and normalize the legacy adapter key.
///
/// This function is synchronous so the deployable binary can call it before
/// tracing, Tokio or worker threads exist. Explicit invalid, empty, non-Unicode
/// or conflicting sources fail closed through the shared parser.
pub fn validate_process_environment() -> Result<ValidatedMatrixAdapterEnvironment, &'static str> {
    let mut values = Vec::with_capacity(PROFILE_ENV_NAMES.len());
    for name in PROFILE_ENV_NAMES {
        match std::env::var(name) {
            Ok(value) => values.push(Some(value)),
            Err(std::env::VarError::NotPresent) => values.push(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("non_unicode_matrix_runtime_profile");
            }
        }
    }
    let profile = resolve_profiles(&values)?;
    std::env::set_var("MATRIX_ENTRY_RUNTIME_PROFILE", profile.legacy_value());
    Ok(ValidatedMatrixAdapterEnvironment { _private: () })
}

/// Public adapter state with construction restricted to validated process input.
pub struct AppState {
    inner: implementation::AppState,
}

impl AppState {
    /// Construct state only after the caller has completed strict synchronous
    /// profile validation. The token is consumed and cannot be reused.
    pub async fn from_validated_env(
        _validated: ValidatedMatrixAdapterEnvironment,
    ) -> Result<Self, String> {
        implementation::AppState::from_env()
            .await
            .map(|inner| Self { inner })
    }

    /// Listener address selected by the fully validated internal configuration.
    pub fn bind_addr(&self) -> &str {
        &self.inner.config().bind_addr
    }
}

/// Build the production router while keeping implementation constructors private.
pub fn build_router(state: AppState) -> Router {
    let reconciliation = result_reconciliation::router(state.inner.config()).layer(
        middleware::from_fn(
            reconciliation_response_binding::enforce_reconciliation_response_binding,
        ),
    );
    let delivery_binding_policy =
        delivery_binding::DeliveryBindingPolicy::from_config(state.inner.config());
    implementation::build_router(state.inner)
        .layer(middleware::from_fn_with_state(
            delivery_binding_policy,
            delivery_binding::enforce_delivery_binding,
        ))
        .merge(reconciliation)
}
