//! Sled storage backend for vault (embedded, single-node)

use async_trait::async_trait;
use std::sync::Arc;
use k1s_storage::SledBackend;

use crate::error::VaultResult;
use super::VaultStorage;

pub struct SledStorage {
    backend: Arc<SledBackend>,
}

impl SledStorage {
    pub fn new(backend: Arc<SledBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl VaultStorage for SledStorage {
    async fn get(&self, key: &str) -> VaultResult<Option<Vec<u8>>> {
        self.backend.get(key).await
            .map_err(|e| crate::error::VaultError::Internal(e.to_string()))
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> VaultResult<()> {
        self.backend.put(key, value).await
            .map_err(|e| crate::error::VaultError::Internal(e.to_string()))
    }

    async fn delete(&self, key: &str) -> VaultResult<()> {
        self.backend.delete(key).await
            .map_err(|e| crate::error::VaultError::Internal(e.to_string()))
    }

    async fn list(&self, prefix: &str) -> VaultResult<Vec<(String, Vec<u8>)>> {
        self.backend.list(prefix).await
            .map_err(|e| crate::error::VaultError::Internal(e.to_string()))
    }
}
