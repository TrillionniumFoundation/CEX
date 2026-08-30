//! Stable public adapter around the reviewed startup-guard implementation.
//!
//! The implementation remains an exact Git blob in `runtime_guard_impl.rs`.
//! This boundary owns the pre-runtime environment preparation required by
//! binaries whose legacy state constructors still read process configuration.

mod implementation {
    include!("runtime_guard_impl.rs");

    #[derive(Debug, Clone, Copy)]
    pub(super) struct PreparedStartup {
        service: ServiceKind,
        profile: RuntimeProfile,
        identity_static_fallback_disabled: bool,
    }

    unsafe fn canonicalize_runtime_profile(profile: RuntimeProfile) {
        let canonical = profile.to_string();

        // SAFETY: callers invoke preparation before constructing Tokio or
        // starting any application worker thread. CEX_RUNTIME_PROFILE becomes
        // the canonical downstream authority, including for implicit-dev use.
        unsafe {
            env::set_var("CEX_RUNTIME_PROFILE", &canonical);
        }

        if env::var_os("APP_ENV").is_some() {
            // SAFETY: same pre-runtime contract as above. Canonicalizing an
            // existing compatibility source prevents alias drift downstream.
            unsafe {
                env::set_var("APP_ENV", &canonical);
            }
        }
    }

    pub(super) unsafe fn prepare_process_environment(
        service: ServiceKind,
    ) -> Result<PreparedStartup, StartupError> {
        let profile = resolve_runtime_profile()?;
        let mut identity_static_fallback_disabled = false;

        if profile.is_production_like() {
            validate_production_posture(service)?;

            if matches!(service, ServiceKind::Identity) {
                install_identity_fail_closed_static_sink()?;
                identity_static_fallback_disabled = true;
            }

            // SAFETY: this function's contract requires execution before any
            // Tokio runtime or application worker thread is created. The value
            // is a non-secret boolean already validated as true above.
            unsafe {
                env::set_var(service.fail_fast_env(), "true");
            }
        }

        // SAFETY: upheld by this function's caller.
        unsafe {
            canonicalize_runtime_profile(profile);
        }

        Ok(PreparedStartup {
            service,
            profile,
            identity_static_fallback_disabled,
        })
    }

    pub(super) unsafe fn canonicalize_runtime_profile_environment(
    ) -> Result<RuntimeProfile, StartupError> {
        let profile = resolve_runtime_profile()?;
        // SAFETY: upheld by this function's caller.
        unsafe {
            canonicalize_runtime_profile(profile);
        }
        Ok(profile)
    }

    pub(super) async fn enforce_prepared(
        prepared: PreparedStartup,
    ) -> Result<StartupReport, StartupError> {
        let mut database_preflight = false;
        if prepared.profile.is_production_like() {
            preflight_database().await?;
            database_preflight = true;
        }

        Ok(StartupReport {
            service: prepared.service,
            profile: prepared.profile,
            database_preflight,
            identity_static_fallback_disabled: prepared.identity_static_fallback_disabled,
        })
    }
}

pub use implementation::{
    env_flag, parse_bool_value, resolve_runtime_profile, RuntimeProfile, ServiceKind, StartupError,
    StartupReport, CONFIG_ERROR_EXIT_CODE,
};

/// Opaque proof that process-wide startup mutations completed before the async
/// runtime was constructed.
#[derive(Debug, Clone, Copy)]
pub struct PreparedStartup(implementation::PreparedStartup);

/// Validate the service posture and install the small process-wide compatibility
/// values required by legacy state constructors.
///
/// # Safety
///
/// Call this exactly once from synchronous `main`, before constructing Tokio,
/// initializing libraries that may create threads, or starting any application
/// worker thread.
pub unsafe fn prepare_process_environment(
    service: ServiceKind,
) -> Result<PreparedStartup, StartupError> {
    // SAFETY: the caller accepts and must uphold the pre-runtime contract above.
    unsafe { implementation::prepare_process_environment(service).map(PreparedStartup) }
}

/// Canonicalize any explicitly configured runtime-profile aliases before Tokio
/// is constructed. This is for standalone workers that already enforce their
/// own service-specific production posture.
///
/// # Safety
///
/// The same pre-runtime, single-threaded contract as
/// `prepare_process_environment` applies.
pub unsafe fn canonicalize_runtime_profile_environment(
) -> Result<RuntimeProfile, StartupError> {
    // SAFETY: the caller accepts and must uphold the pre-runtime contract above.
    unsafe { implementation::canonicalize_runtime_profile_environment() }
}

/// Complete the database readiness proof without mutating process environment.
pub async fn enforce_prepared(
    prepared: PreparedStartup,
) -> Result<StartupReport, StartupError> {
    implementation::enforce_prepared(prepared.0).await
}

/// Build the multi-thread runtime used by service binaries after process
/// environment preparation is complete.
pub fn build_multi_thread_runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
}

/// Compatibility entrypoint for non-production callers.
///
/// Production-like callers must use the synchronous preparation contract above;
/// failing closed here prevents process-environment mutation after Tokio has
/// already started.
pub async fn enforce(service: ServiceKind) -> Result<StartupReport, StartupError> {
    let profile = resolve_runtime_profile()?;
    if profile.is_production_like() {
        return Err(StartupError {
            code: "runtime_environment_not_prepared",
            message: format!(
                "{service} must call prepare_process_environment before constructing Tokio"
            ),
        });
    }
    implementation::enforce(service).await
}
