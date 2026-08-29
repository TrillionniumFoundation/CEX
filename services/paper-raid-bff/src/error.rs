use axum::{
    http::StatusCode,
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
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "dependency_unavailable"),
            Self::Upstream => (StatusCode::BAD_GATEWAY, "upstream_rejected"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "paper raid BFF request failed");
        }
        (status, Json(ErrorBody { error: public })).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "BFF database operation failed");
        Self::Internal
    }
}
