use paper_raid_bff::{
    app,
    config::Config,
    probe::{self, ProbeTarget},
};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--probe-health") => {
            std::process::exit(if probe::run(ProbeTarget::Health).await {
                0
            } else {
                1
            });
        }
        Some("--probe-ready") => {
            std::process::exit(if probe::run(ProbeTarget::Ready).await {
                0
            } else {
                1
            });
        }
        Some(_) => {
            eprintln!("unsupported argument");
            std::process::exit(2);
        }
        None => {}
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "paper_raid_bff=info".into()),
        )
        .init();

    let config = Config::from_env().expect("load strict Paper Raid BFF configuration");
    let bind = config.bind;
    let state = app::AppState::connect(config)
        .await
        .expect("initialize Paper Raid BFF");
    let listener = TcpListener::bind(bind)
        .await
        .expect("bind Paper Raid BFF listener");
    info!(%bind, "Paper Raid BFF listening");
    axum::serve(listener, app::router(state))
        .await
        .expect("serve Paper Raid BFF");
}
