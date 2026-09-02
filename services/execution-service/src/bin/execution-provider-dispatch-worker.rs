use execution_service::{provider_dispatch, validate_internal_service_auth};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;

fn reject(message: &str) -> ! {
    eprintln!("execution-provider-dispatch-worker startup rejected: {message}");
    std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
}

fn main() {
    // SAFETY: process-wide environment preparation runs before tracing, Tokio,
    // or any worker thread is initialized.
    let prepared =
        match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Execution) } {
            Ok(prepared) => prepared,
            Err(error) => reject(&error.to_string()),
        };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => reject(&format!("async runtime initialization failed: {error}")),
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => reject(&error.to_string()),
    };

    if startup.profile.is_production_like() {
        reject(
            "legacy local provider dispatch is forbidden in beta, staging, and production; participating Agents must execute externally through hepta_agent_protocol_v1",
        );
    }
    if !runtime_guard::env_flag("CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH", false) {
        reject(
            "set CEX_ENABLE_LEGACY_LOCAL_PROVIDER_DISPATCH=true only in an isolated local/test environment; the default and authoritative runtime policy is external_only",
        );
    }
    if let Err(error) = validate_internal_service_auth(false) {
        reject(&error);
    }

    eprintln!(
        "execution-provider-dispatch-worker accepted non-production compatibility profile={} db_preflight={} runtime_policy=legacy_local_only",
        startup.profile, startup.database_preflight
    );
    if let Err(error) = provider_dispatch::run_worker_from_env().await {
        eprintln!("execution-provider-dispatch-worker failed: {error}");
        std::process::exit(1);
    }
}
