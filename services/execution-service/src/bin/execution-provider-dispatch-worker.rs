use execution_service::{provider_dispatch, validate_internal_service_auth};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared =
        match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Execution) } {
            Ok(prepared) => prepared,
            Err(error) => {
                eprintln!("provider-dispatch-worker startup rejected: {error}");
                std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
            }
        };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("provider-dispatch-worker async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("provider-dispatch-worker startup rejected: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    if let Err(error) = validate_internal_service_auth(startup.profile.is_production_like()) {
        eprintln!("provider-dispatch-worker startup rejected: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }

    eprintln!(
        "provider-dispatch-worker startup accepted profile={} db_preflight={}",
        startup.profile, startup.database_preflight
    );
    if let Err(error) = provider_dispatch::run_worker_from_env().await {
        eprintln!("provider-dispatch-worker terminated: {error}");
        std::process::exit(1);
    }
}
