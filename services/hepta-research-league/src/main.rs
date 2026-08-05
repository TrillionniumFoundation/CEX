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

    if std::env::args().skip(1).eq(["--probe-ready"]) {
        return match probe_ready().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Hepta readiness probe failed: {error}");
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
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("HEPTA_DATABASE_URL or DATABASE_URL must be set");
    let state = AppState::connect(&database_url, security)
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
