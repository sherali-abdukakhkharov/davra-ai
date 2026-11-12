use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use davra_core::api::ApiError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("missing or invalid token")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    TooMany(String),
    #[error("{0}")]
    RateLimited(String),
    #[error("{0}")]
    Conflict(String),
    #[error("wrong e-mail or password")]
    InvalidCredentials,
    /// 400 tied to one request field, so a form can show it in place.
    #[error("{1}")]
    Field(&'static str, String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::TooMany(_) => (StatusCode::TOO_MANY_REQUESTS, "loop_guard"),
            AppError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "email_taken"),
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),
            AppError::Field(..) => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let message = match &self {
            AppError::Internal(_) => "internal error".to_string(),
            other => other.to_string(),
        };
        let mut resp = (
            status,
            Json(ApiError {
                error: code.into(),
                message,
                field: match &self {
                    AppError::Field(f, _) => Some((*f).to_string()),
                    AppError::Conflict(_) => Some("email".to_string()),
                    _ => None,
                },
            }),
        )
            .into_response();
        if matches!(self, AppError::RateLimited(_)) {
            resp.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("900"),
            );
        }
        resp
    }
}

pub type AppResult<T> = Result<T, AppError>;
