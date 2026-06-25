//! The server's HTTP error → status + `{error}` JSON, matching the web contract: 401 for missing/bad
//! auth, 401 "Invalid email or password." for login failures, 400 for content/validation failures
//! (where the web's thrown mutation errors surface), 409 dup, 404, 500.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    /// Missing or invalid bearer token on a protected endpoint.
    Unauthorized,
    /// Login with bad credentials (distinct message from the generic 401).
    InvalidCredentials,
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Internal(String),
}

impl AppError {
    /// Wrap any error (pool/join/infra) as a 500.
    pub fn internal(e: impl std::fmt::Display) -> Self {
        AppError::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized.".to_string()),
            AppError::InvalidCredentials => {
                (StatusCode::UNAUTHORIZED, "Invalid email or password.".to_string())
            }
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

/// vegify-core failures are content/owner-guard errors → 400 with the message, mirroring the web
/// (a thrown mutation error surfaces as `400 {error}`).
impl From<vegify_core::Error> for AppError {
    fn from(e: vegify_core::Error) -> Self {
        match e {
            vegify_core::Error::Db(m) | vegify_core::Error::Auth(m) => AppError::BadRequest(m),
        }
    }
}

/// A raw SQLite failure in the server's own queries (auth) is an internal error.
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Internal(e.to_string())
    }
}
