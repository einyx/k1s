//! Vault error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Secret not found: {0}")]
    SecretNotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Certificate generation failed: {0}")]
    CertificateFailed(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<k1s_storage::StorageError> for VaultError {
    fn from(err: k1s_storage::StorageError) -> Self {
        VaultError::Storage(err.to_string())
    }
}

pub type VaultResult<T> = Result<T, VaultError>;
