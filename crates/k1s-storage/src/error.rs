//! Storage error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Key already exists: {0}")]
    AlreadyExists(String),

    #[error("Revision mismatch: expected {expected}, got {actual}")]
    RevisionMismatch { expected: i64, actual: i64 },

    #[error("Storage backend error: {0}")]
    Backend(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Watch closed")]
    WatchClosed,

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<sled::Error> for StorageError {
    fn from(err: sled::Error) -> Self {
        StorageError::Backend(err.to_string())
    }
}

pub type StorageResult<T> = Result<T, StorageError>;
