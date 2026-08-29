use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("request conflict: {0}")]
    Conflict(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("dependency unavailable: {0}")]
    Unavailable(&'static str),
    #[error("request rate limited")]
    RateLimited { retry_after_secs: u64 },
    #[error("upstream rejected the request")]
    Upstream,
    #[error("not found")]
    NotFound,
    #[error("internal service error")]
    Internal,
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, public) = match &self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication_required"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            // This one transport conflict is intentionally machine-readable:
            // local clients may refresh CSRF and replay the same idempotent
            // command. All domain conflicts remain opaque at this boundary.
            Self::Conflict(code) if code == "csrf_replayed" => {
                (StatusCode::CONFLICT, "csrf_replayed")
            }
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "dependency_unavailable"),
            Self::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::Upstream => (StatusCode::BAD_GATEWAY, "upstream_rejected"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "paper raid BFF request failed");
        }
        let mut response = (status, Json(ErrorBody { error: public })).into_response();
        if let Self::RateLimited { retry_after_secs } = self {
            if let Ok(value) = HeaderValue::from_str(&retry_after_secs.max(1).to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "BFF database operation failed");
        Self::Internal
    }
}
