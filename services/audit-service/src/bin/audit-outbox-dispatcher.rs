#[path = "../../../../crates/shared-config/src/service_client.rs"]
mod service_client;

use audit_service::outbox_dispatcher::{dispatch_once, DispatcherConfig, DISPATCHER_SERVICE_ID};
use shared_config::runtime_guard::{self, ServiceKind};
use shared_tracing::init_tracing;
use sqlx::postgres::PgPoolOptions;
use std::{env, process, time::Duration};

fn main() {
    // SAFETY: this is the first startup action, before tracing, Tokio, or any
    // application worker thread is initialized.
    let prepared = match unsafe { runtime_guard::prepare_process_environment(ServiceKind::Audit) } {
        Ok(prepared) => prepared,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} startup rejected: {error}");
            process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    init_tracing();
    let runtime = match runtime_guard::build_multi_thread_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} async runtime initialization failed: {error}");
            process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };
    runtime.block_on(run(prepared));
}

async fn run(prepared: runtime_guard::PreparedStartup) {
    let startup = match runtime_guard::enforce_prepared(prepared).await {
        Ok(startup) => startup,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} startup rejected: {error}");
            process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    let config = match DispatcherConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} startup rejected: {error}");
            process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    let client = match service_client::build_internal_http_client(
        DISPATCHER_SERVICE_ID,
        startup.profile.is_production_like(),
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} startup rejected: {error}");
            process::exit(runtime_guard::CONFIG_ERROR_EXIT_CODE);
        }
    };

    let pool = match PgPoolOptions::new()
        .max_connections(4)
        .connect(&config.database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("{DISPATCHER_SERVICE_ID} database connection failed: {error}");
            process::exit(1);
        }
    };

    let run_once = env_flag("CEX_AUDIT_OUTBOX_RUN_ONCE")
        || env::args().skip(1).any(|argument| argument == "--once");

    eprintln!(
        "{DISPATCHER_SERVICE_ID} started profile={} worker_id={} batch_size={} lease_seconds={} run_once={}",
        startup.profile,
        config.worker_id,
        config.batch_size,
        config.lease_seconds,
        run_once
    );

    loop {
        match dispatch_once(&pool, &client, &config).await {
            Ok(stats) => {
                eprintln!(
                    "{DISPATCHER_SERVICE_ID} batch claimed={} delivered={} retry_scheduled={} dead_lettered={}",
                    stats.claimed,
                    stats.delivered,
                    stats.retry_scheduled,
                    stats.dead_lettered
                );
                if run_once {
                    return;
                }
                if stats.claimed == 0 {
                    tokio::time::sleep(config.poll_interval).await;
                } else {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
            Err(error) => {
                eprintln!("{DISPATCHER_SERVICE_ID} batch failed: {error}");
                if run_once {
                    process::exit(1);
                }
                tokio::time::sleep(config.poll_interval).await;
            }
        }
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
