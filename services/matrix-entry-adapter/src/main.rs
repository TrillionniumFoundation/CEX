use axum::Router;
use matrix_entry_adapter::{
    build_router, validate_process_environment, AppState, ValidatedMatrixAdapterEnvironment,
};
use shared_tracing::init_tracing;

fn prepare_runtime_profile() -> Result<ValidatedMatrixAdapterEnvironment, &'static str> {
    validate_process_environment()
}

fn main() {
    let validated_environment = match prepare_runtime_profile() {
        Ok(validated_environment) => validated_environment,
        Err(code) => {
            eprintln!("{code}");
            std::process::exit(78);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            eprintln!("matrix_adapter_runtime_initialization_failed");
            std::process::exit(78);
        }
    };
    if runtime.block_on(run(validated_environment)).is_err() {
        eprintln!("matrix_adapter_startup_or_serve_failed");
        std::process::exit(2);
    }
}

async fn run(validated_environment: ValidatedMatrixAdapterEnvironment) -> std::io::Result<()> {
    init_tracing();
    let state = AppState::from_validated_env(validated_environment)
        .await
        .map_err(std::io::Error::other)?;
    let bind_addr = state.bind_addr().to_owned();
    let app: Router = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    axum::serve(listener, app).await
}
