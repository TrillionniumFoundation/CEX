use execution_service::{provider_dispatch, validate_internal_service_auth};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();

    let startup = match runtime_guard::enforce(ServiceKind::Execution).await {
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
