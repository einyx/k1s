//! Storage backends for vault

pub mod sled_backend;
pub mod etcd_backend;

use async_trait::async_trait;
use crate::error::VaultResult;

/// Storage backend abstraction for vault
#[async_trait]
pub trait VaultStorage: Send + Sync {
    /// Get a value by key
    async fn get(&self, key: &str) -> VaultResult<Option<Vec<u8>>>;

    /// Put a value
    async fn put(&self, key: &str, value: Vec<u8>) -> VaultResult<()>;

    /// Delete a value
    async fn delete(&self, key: &str) -> VaultResult<()>;

    /// List keys with prefix
    async fn list(&self, prefix: &str) -> VaultResult<Vec<(String, Vec<u8>)>>;

    /// Watch for changes (optional, for real-time sync)
    async fn watch(&self, prefix: &str) -> VaultResult<Box<dyn futures::Stream<Item = WatchEvent> + Send + Unpin>> {
        // Default: not implemented
        Err(crate::error::VaultError::Internal("Watch not supported".to_string()))
    }
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Put { key: String, value: Vec<u8> },
    Delete { key: String },
}

/// Storage backend type
pub enum StorageBackend {
    Sled(sled_backend::SledStorage),
    Etcd(etcd_backend::EtcdStorage),
}

impl StorageBackend {
    pub fn as_trait(&self) -> &dyn VaultStorage {
        match self {
            StorageBackend::Sled(s) => s,
            StorageBackend::Etcd(e) => e,
        }
    }
}
