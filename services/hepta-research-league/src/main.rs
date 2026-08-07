use hepta_research_league::{app, AppState, SecurityConfig};
use serde_json::Value;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    process::ExitCode,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tracing::info;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_MAX_RESPONSE_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandMode {
    Serve,
    ProbeReady,
    Migrate,
}

fn command_mode(arguments: &[String]) -> Result<CommandMode, String> {
    match arguments {
        [] => Ok(CommandMode::Serve),
        [argument] if argument == "--probe-ready" => Ok(CommandMode::ProbeReady),
        [argument] if argument == "--migrate" => Ok(CommandMode::Migrate),
        _ => Err("usage: hepta-research-league [--probe-ready|--migrate]".to_string()),
    }
}

fn migration_database_url_from_file(path: &str) -> Result<String, String> {
    let secret = std::fs::read_to_string(path)
        .map_err(|error| format!("read HEPTA_MIGRATION_DATABASE_URL_FILE: {error}"))?;
    normalize_migration_database_url_secret(&secret)
}

fn normalize_migration_database_url_secret(secret: &str) -> Result<String, String> {
    let database_url = secret.trim();
    if database_url.is_empty() {
        return Err(
            "HEPTA_MIGRATION_DATABASE_URL_FILE must contain a nonempty PostgreSQL URL".to_string(),
        );
    }
    Ok(database_url.to_string())
}

fn probe_target(bind_addr: &str) -> Result<SocketAddr, String> {
    let configured: SocketAddr = bind_addr
        .parse()
        .map_err(|error| format!("HEPTA_BIND_ADDR is not a socket address: {error}"))?;
    let ip = match configured.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    Ok(SocketAddr::new(ip, configured.port()))
}

async fn probe_ready_at(target: SocketAddr) -> Result<(), String> {
    let mut stream = timeout(PROBE_TIMEOUT, TcpStream::connect(target))
        .await
        .map_err(|_| "readiness probe connection timed out".to_string())?
        .map_err(|error| format!("readiness probe connection failed: {error}"))?;
    timeout(
        PROBE_TIMEOUT,
        stream.write_all(b"GET /ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
    )
    .await
    .map_err(|_| "readiness probe request timed out".to_string())?
    .map_err(|error| format!("readiness probe request failed: {error}"))?;
    let mut response = Vec::new();
    timeout(
        PROBE_TIMEOUT,
        stream
            .take(PROBE_MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut response),
    )
    .await
    .map_err(|_| "readiness probe response timed out".to_string())?
    .map_err(|error| format!("readiness probe response failed: {error}"))?;
    if response.len() as u64 > PROBE_MAX_RESPONSE_BYTES {
        return Err("readiness probe response exceeds size limit".to_string());
    }
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|offset| offset + 4)
        .ok_or_else(|| "readiness probe response has no HTTP header boundary".to_string())?;
    let status_line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(|| "readiness probe response has no HTTP status line".to_string())?;
    let status_line = std::str::from_utf8(&response[..status_line_end])
        .map_err(|_| "readiness probe status line is not UTF-8".to_string())?;
    if status_line != "HTTP/1.1 200 OK" && status_line != "HTTP/1.0 200 OK" {
        return Err(format!("readiness endpoint returned {status_line}"));
    }
    let headers = std::str::from_utf8(&response[..header_end - 4])
        .map_err(|_| "readiness probe headers are not UTF-8".to_string())?;
    let content_types = headers
        .split("\r\n")
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    if content_types.as_slice() != ["application/json"] {
        return Err(
            "readiness endpoint must return one exact Content-Type: application/json".to_string(),
        );
    }
    let body: Value = serde_json::from_slice(&response[header_end..])
        .map_err(|error| format!("readiness probe response is not strict JSON: {error}"))?;
    if body.get("ready").and_then(Value::as_bool) != Some(true) {
        return Err("readiness endpoint did not report ready=true".to_string());
    }
    Ok(())
}

async fn probe_ready() -> Result<(), String> {
    let bind_addr =
        std::env::var("HEPTA_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7011".to_string());
    probe_ready_at(probe_target(&bind_addr)?).await
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hepta_research_league=info".into()),
        )
        .init();

    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let command_mode = match command_mode(&arguments) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    if command_mode == CommandMode::ProbeReady {
        return match probe_ready().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Hepta readiness probe failed: {error}");
                ExitCode::FAILURE
            }
        };
    }

    if command_mode == CommandMode::Migrate {
        let migration_database_url_file = std::env::var("HEPTA_MIGRATION_DATABASE_URL_FILE")
            .expect(
                "HEPTA_MIGRATION_DATABASE_URL_FILE must name the one-shot migrator secret file",
            );
        let migration_database_url = migration_database_url_from_file(&migration_database_url_file)
            .expect("load one-shot migration-owner PostgreSQL URL");
        let runtime_role = std::env::var("HEPTA_RUNTIME_DATABASE_ROLE")
            .expect("HEPTA_RUNTIME_DATABASE_ROLE must name the ordinary runtime login role");
        let finality_role = std::env::var("HEPTA_FINALITY_DATABASE_ROLE")
            .expect("HEPTA_FINALITY_DATABASE_ROLE must name the isolated finality login role");
        return match AppState::migrate_and_configure_database_roles(
            &migration_database_url,
            &runtime_role,
            &finality_role,
        )
        .await
        {
            Ok(()) => {
                info!("Hepta migrations and database-role grants completed");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Hepta one-shot migration failed: {error}");
                ExitCode::FAILURE
            }
        };
    }

    let bind_addr =
        std::env::var("HEPTA_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:7011".to_string());
    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("bind Hepta Research League listener");
    info!(%bind_addr, "Hepta Research League listening");
    let security = SecurityConfig::from_env().expect("load Hepta service authentication");
    let database_url = std::env::var("HEPTA_DATABASE_URL")
        .expect("HEPTA_DATABASE_URL must use the ordinary runtime database role");
    let finality_database_url = std::env::var("HEPTA_FINALITY_DATABASE_URL")
        .expect("HEPTA_FINALITY_DATABASE_URL must use the isolated finality database role");
    let state =
        AppState::connect_with_database_roles(&database_url, &finality_database_url, security)
            .await
            .expect("initialize durable Hepta repository")
            .with_nakama_control_http_from_env()
            .expect("configure signed Nakama control client");
    axum::serve(listener, app(state))
        .await
        .expect("serve Hepta Research League");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_modes_are_exact_and_fail_closed() {
        assert_eq!(command_mode(&[]).unwrap(), CommandMode::Serve);
        assert_eq!(
            command_mode(&["--probe-ready".to_string()]).unwrap(),
            CommandMode::ProbeReady
        );
        assert_eq!(
            command_mode(&["--migrate".to_string()]).unwrap(),
            CommandMode::Migrate
        );
        assert!(command_mode(&["--migrate=true".to_string()]).is_err());
        assert!(command_mode(&["--migrate".to_string(), "extra".to_string()]).is_err());
    }

    #[test]
    fn migration_database_url_secret_is_trimmed_and_nonempty() {
        assert_eq!(
            normalize_migration_database_url_secret("postgresql://owner@db/hepta\n").unwrap(),
            "postgresql://owner@db/hepta"
        );
        assert!(normalize_migration_database_url_secret(" \n\t").is_err());
    }

    #[test]
    fn probe_maps_wildcard_listener_to_loopback() {
        assert_eq!(
            probe_target("0.0.0.0:7011").unwrap(),
            "127.0.0.1:7011".parse().unwrap()
        );
        assert_eq!(
            probe_target("[::]:7011").unwrap(),
            "[::1]:7011".parse().unwrap()
        );
    }
}
