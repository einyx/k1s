//! Sled-based etcd-compatible storage for k1s
//!
//! Provides a key-value storage backend with:
//! - MVCC (Multi-Version Concurrency Control)
//! - Watch streams for real-time updates
//! - Prefix-based listing
//! - Atomic transactions

pub mod backend;
pub mod encryption;
pub mod error;
pub mod watch;

pub use backend::{SledBackend, ResourceStore};
pub use encryption::SecretEncryption;
pub use error::{StorageError, StorageResult};
pub use watch::{WatchEvent, WatchEventType, Watcher};

use async_trait::async_trait;
use k1s_types::Resource;

/// Storage backend trait
#[async_trait]
pub trait Storage: Send + Sync + 'static {
    /// Get a value by key
    async fn get(&self, key: &str) -> StorageResult<Option<Vec<u8>>>;

    /// Put a value, returning the new revision
    async fn put(&self, key: &str, value: Vec<u8>) -> StorageResult<i64>;

    /// Delete a key, returning true if it existed
    async fn delete(&self, key: &str) -> StorageResult<bool>;

    /// List all keys with a given prefix
    async fn list(&self, prefix: &str) -> StorageResult<Vec<(String, Vec<u8>)>>;

    /// Watch for changes to keys with a given prefix
    async fn watch(&self, prefix: &str, revision: i64) -> StorageResult<Watcher>;

    /// Get the current revision
    async fn revision(&self) -> StorageResult<i64>;

    /// Compact old revisions up to the given revision
    async fn compact(&self, revision: i64) -> StorageResult<()>;
}

/// Type-safe storage operations for Kubernetes resources
#[async_trait]
pub trait ResourceStorage<R: Resource>: Send + Sync {
    /// Get a resource by name
    async fn get(&self, namespace: Option<&str>, name: &str) -> StorageResult<Option<R>>;

    /// List all resources
    async fn list(&self, namespace: Option<&str>) -> StorageResult<Vec<R>>;

    /// Create a new resource
    async fn create(&self, resource: &R) -> StorageResult<R>;

    /// Update an existing resource
    async fn update(&self, resource: &R) -> StorageResult<R>;

    /// Delete a resource
    async fn delete(&self, namespace: Option<&str>, name: &str) -> StorageResult<bool>;

    /// Watch for changes
    async fn watch(&self, namespace: Option<&str>) -> StorageResult<Watcher>;
}
