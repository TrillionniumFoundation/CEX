use execution_service::settlement_worker;
use shared_config::runtime_guard;
use shared_tracing::init_tracing;

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized. The worker enforces its own
    // service-specific production posture after profile canonicalization.
    if let Err(error) = unsafe { runtime_guard::canonicalize_runtime_profile_environment() } {
        eprintln!("execution-settlement-worker startup rejected: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("execution-settlement-worker async runtime initialization failed: {error}");
            std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run());
}

async fn run() {
    if let Err(error) = settlement_worker::run_from_env().await {
        eprintln!("execution-settlement-worker terminated: {error}");
        std::process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
    }
}
