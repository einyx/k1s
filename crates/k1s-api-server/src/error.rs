//! API error types and responses

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Resource not found: {kind}/{name}")]
    NotFound { kind: String, name: String },

    #[error("Resource already exists: {kind}/{name}")]
    AlreadyExists { kind: String, name: String },

    #[error("Conflict: resource version mismatch")]
    Conflict,

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Storage error: {0}")]
    Storage(#[from] k1s_storage::StorageError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_response) = match &self {
            ApiError::NotFound { kind, name } => (
                StatusCode::NOT_FOUND,
                ErrorResponse::new(
                    StatusCode::NOT_FOUND,
                    "NotFound",
                    &format!("{} \"{}\" not found", kind, name),
                ),
            ),
            ApiError::AlreadyExists { kind, name } => (
                StatusCode::CONFLICT,
                ErrorResponse::new(
                    StatusCode::CONFLICT,
                    "AlreadyExists",
                    &format!("{} \"{}\" already exists", kind, name),
                ),
            ),
            ApiError::Conflict => (
                StatusCode::CONFLICT,
                ErrorResponse::new(
                    StatusCode::CONFLICT,
                    "Conflict",
                    "the object has been modified; please apply your changes to the latest version",
                ),
            ),
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(StatusCode::BAD_REQUEST, "BadRequest", msg),
            ),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                ErrorResponse::new(StatusCode::UNAUTHORIZED, "Unauthorized", "unauthorized"),
            ),
            ApiError::Forbidden => (
                StatusCode::FORBIDDEN,
                ErrorResponse::new(StatusCode::FORBIDDEN, "Forbidden", "forbidden"),
            ),
            ApiError::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid", msg),
            ),
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", msg),
            ),
            ApiError::Storage(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "StorageError",
                    &err.to_string(),
                ),
            ),
            ApiError::Serialization(err) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(StatusCode::BAD_REQUEST, "SerializationError", &err.to_string()),
            ),
        };

        (status, Json(error_response)).into_response()
    }
}

/// Kubernetes-style error response
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub api_version: String,
    pub kind: String,
    pub status: String,
    pub message: String,
    pub reason: String,
    pub code: u16,
}

impl ErrorResponse {
    pub fn new(status: StatusCode, reason: &str, message: &str) -> Self {
        Self {
            api_version: "v1".to_string(),
            kind: "Status".to_string(),
            status: "Failure".to_string(),
            message: message.to_string(),
            reason: reason.to_string(),
            code: status.as_u16(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
