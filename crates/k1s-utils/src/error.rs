//! Common error types for k1s

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Resource not found: {kind}/{name}")]
    NotFound { kind: String, name: String },

    #[error("Resource already exists: {kind}/{name}")]
    AlreadyExists { kind: String, name: String },

    #[error("Conflict: resource version mismatch")]
    Conflict,

    #[error("Invalid resource: {0}")]
    Invalid(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;
