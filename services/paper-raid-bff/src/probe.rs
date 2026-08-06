use std::time::Duration;

use reqwest::{header, Client, StatusCode};

const MAX_PROBE_BODY: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeTarget {
    Health,
    Ready,
}

impl ProbeTarget {
    fn path(self) -> &'static str {
        match self {
            Self::Health => "/health",
            Self::Ready => "/ready",
        }
    }
}

pub async fn run(target: ProbeTarget) -> bool {
    let port = match std::env::var("PAPER_RAID_BFF_PROBE_PORT") {
        Ok(value) => match value.parse::<u16>() {
            Ok(port) if port != 0 => port,
            _ => return false,
        },
        Err(_) => 7020,
    };
    run_at(target, port).await
}

async fn run_at(target: ProbeTarget, port: u16) -> bool {
    let Ok(client) = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(1))
        .timeout(Duration::from_secs(2))
        .user_agent("paper-raid-bff-probe/0.1")
        .build()
    else {
        return false;
    };
    let url = format!("http://127.0.0.1:{port}{}", target.path());
    let Ok(mut response) = client.get(url).send().await else {
        return false;
    };
    if response.status() != StatusCode::OK
        || response.status().is_redirection()
        || !response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                matches!(
                    value,
                    "application/json" | "application/json; charset=utf-8"
                )
            })
        || response
            .content_length()
            .is_some_and(|length| length > MAX_PROBE_BODY as u64)
    {
        return false;
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len().saturating_add(chunk.len()) <= MAX_PROBE_BODY => {
                body.extend_from_slice(&chunk);
            }
            Ok(Some(_)) | Err(_) => return false,
            Ok(None) => break,
        }
    }
    serde_json::from_slice::<serde_json::Value>(&body).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::StatusCode as AxumStatus,
        response::{IntoResponse, Redirect, Response},
        routing::get,
        Json, Router,
    };

    async fn spawn(router: Router) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind probe mock");
        let port = listener.local_addr().expect("probe address").port();
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("probe mock");
        });
        port
    }

    async fn ok() -> Json<serde_json::Value> {
        Json(serde_json::json!({"status":"ok"}))
    }

    async fn redirect() -> Redirect {
        Redirect::temporary("/health")
    }

    async fn wrong_type() -> Response {
        (AxumStatus::OK, [(header::CONTENT_TYPE, "text/plain")], "{}").into_response()
    }

    async fn oversized() -> Json<serde_json::Value> {
        Json(serde_json::json!({"payload":"x".repeat(MAX_PROBE_BODY + 1)}))
    }

    #[tokio::test]
    async fn fixed_loopback_probe_accepts_only_bounded_json_without_redirect() {
        let good = spawn(Router::new().route("/health", get(ok))).await;
        assert!(run_at(ProbeTarget::Health, good).await);
        for router in [
            Router::new().route("/health", get(redirect)),
            Router::new().route("/health", get(wrong_type)),
            Router::new().route("/health", get(oversized)),
        ] {
            assert!(!run_at(ProbeTarget::Health, spawn(router).await).await);
        }
    }
}
