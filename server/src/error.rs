//! Application error type and its mapping to HTTP responses.

use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde_json::json;

/// All errors that can surface from a request handler.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("{0}")]
    Validation(String),

    #[error("authentication required")]
    Unauthorized,

    #[error("you do not have access to this resource")]
    Forbidden,

    #[error("not found")]
    NotFound,

    #[error("{0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// A short, stable machine-readable error code for the JSON body.
    fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) => "validation_error",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden => "forbidden",
            AppError::NotFound => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Db(_) | AppError::Pool(_) | AppError::Internal(_) => "internal_error",
        }
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Db(_) | AppError::Pool(_) | AppError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Log the full detail server-side; return a sanitized body to clients.
        if self.status_code().is_server_error() {
            log::error!("{self}");
        }
        let message = if self.status_code().is_server_error() {
            "internal server error".to_string()
        } else {
            self.to_string()
        };
        HttpResponse::build(self.status_code())
            .json(json!({ "error": self.code(), "message": message }))
    }
}
