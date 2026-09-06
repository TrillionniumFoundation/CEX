mod runtime;

use capability_service::build_router;
use runtime::RuntimeConfig;
use shared_tracing::init_tracing;

const CONFIG_ERROR_EXIT_CODE: i32 = 78;

#[tokio::main]
async fn main() {
    init_tracing();

    let runtime = match RuntimeConfig::from_env() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("capability-service startup rejected: {error}");
            std::process::exit(CONFIG_ERROR_EXIT_CODE);
        }
    };
    eprintln!(
        "capability-service startup accepted profile={} registry_source={} bind_addr={}",
        runtime.profile(),
        runtime.registry_source(),
        runtime.bind_addr()
    );

    let state = runtime.build_state().await;
    let app = build_router(state);
    let listener = match tokio::net::TcpListener::bind(runtime.bind_addr()).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("capability-service bind failed: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("capability-service server failed: {error}");
        std::process::exit(1);
    }
}
