use execution_service::settlement_worker;
use shared_tracing::init_tracing;

#[tokio::main]
async fn main() {
    init_tracing();
    if let Err(error) = settlement_worker::run_from_env().await {
        eprintln!("execution-settlement-worker terminated: {error}");
        std::process::exit(78);
    }
}
