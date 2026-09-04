mod runtime_profile;

use axum::Router;
use matrix_entry_adapter::{build_router, AppState};
use runtime_profile::resolve_profiles;
use shared_tracing::init_tracing;

fn prepare_runtime_profile() -> Result<(), &'static str> {
    let mut values = Vec::new();
    for name in ["MATRIX_ENTRY_RUNTIME_PROFILE", "CEX_RUNTIME_PROFILE", "APP_ENV"] {
        match std::env::var(name) {
            Ok(value) => values.push(Some(value)),
            Err(std::env::VarError::NotPresent) => values.push(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("non_unicode_matrix_runtime_profile");
            }
        }
    }
    let profile = resolve_profiles(&values)?;
    // This is the first operation in synchronous main, before tracing, Tokio,
    // state constructors or worker threads. Preserve the global profile, but
    // normalize the legacy adapter-specific source to a non-weaker policy.
    std::env::set_var("MATRIX_ENTRY_RUNTIME_PROFILE", profile.legacy_value());
    Ok(())
}

fn main() {
    if let Err(code) = prepare_runtime_profile() {
        eprintln!("{code}");
        std::process::exit(78);
    }
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
    if runtime.block_on(run()).is_err() {
        eprintln!("matrix_adapter_startup_or_serve_failed");
        std::process::exit(2);
    }
}

async fn run() -> std::io::Result<()> {
    init_tracing();
    let state = AppState::from_env().await.map_err(std::io::Error::other)?;
    let app: Router = build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(&state.config().bind_addr).await?;
    axum::serve(listener, app).await
}
