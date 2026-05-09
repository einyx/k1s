//! Sled-based storage backend with MVCC support

use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use sled::{Db, Tree};
use tokio::sync::RwLock;

use crate::encryption::SecretEncryption;
use crate::error::{StorageError, StorageResult};
use crate::watch::{WatchBroadcaster, WatchEvent, Watcher};
use crate::Storage;

const DATA_TREE: &str = "data";
const REVISION_TREE: &str = "revisions";
const HISTORY_TREE: &str = "history";

/// Sled-based storage backend
pub struct SledBackend {
    db: Db,
    data: Tree,
    revisions: Tree,
    history: Tree,
    current_revision: AtomicI64,
    watcher: WatchBroadcaster,
    _lock: Arc<RwLock<()>>,
    encryption: Option<Arc<SecretEncryption>>,
}

impl SledBackend {
    /// Open or create a new storage at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> StorageResult<Self> {
        let db = sled::open(path)?;
        Self::from_db(db)
    }

    /// Create an in-memory storage (for testing)
    pub fn in_memory() -> StorageResult<Self> {
        let config = sled::Config::new().temporary(true);
        let db = config.open()?;
        Self::from_db(db)
    }

    fn from_db(db: Db) -> StorageResult<Self> {
        let data = db.open_tree(DATA_TREE)?;
        let revisions = db.open_tree(REVISION_TREE)?;
        let history = db.open_tree(HISTORY_TREE)?;

        // Load current revision
        let current_revision = revisions
            .get(b"current")?
            .map(|v| i64::from_be_bytes(v.as_ref().try_into().unwrap_or([0; 8])))
            .unwrap_or(0);

        Ok(Self {
            db,
            data,
            revisions,
            history,
            current_revision: AtomicI64::new(current_revision),
            watcher: WatchBroadcaster::default(),
            _lock: Arc::new(RwLock::new(())),
            encryption: None,
        })
    }

    /// Enable encryption for secrets
    pub fn with_encryption(mut self, encryption: SecretEncryption) -> Self {
        self.encryption = Some(Arc::new(encryption));
        self
    }

    /// Check if a key is for a secret resource
    fn is_secret_key(key: &str) -> bool {
        key.contains("/v1/secrets/") || key.contains("/v1, Resource=secrets")
    }

    /// Encrypt data if it's a secret
    fn maybe_encrypt(&self, key: &str, data: &[u8]) -> StorageResult<Vec<u8>> {
        if Self::is_secret_key(key) {
            if let Some(enc) = &self.encryption {
                let encrypted = enc.encrypt(data)
                    .map_err(|e| StorageError::Internal(e.to_string()))?;
                Ok(encrypted.into_bytes())
            } else {
                // No encryption configured, store as-is
                Ok(data.to_vec())
            }
        } else {
            Ok(data.to_vec())
        }
    }

    /// Decrypt data if it's a secret
    fn maybe_decrypt(&self, key: &str, data: &[u8]) -> StorageResult<Vec<u8>> {
        if Self::is_secret_key(key) {
            if let Some(enc) = &self.encryption {
                // Try to decrypt - if it fails, data might not be encrypted yet
                let data_str = std::str::from_utf8(data)
                    .map_err(|e| StorageError::Internal(format!("Invalid UTF-8: {e}")))?;

                match enc.decrypt(data_str) {
                    Ok(decrypted) => Ok(decrypted),
                    Err(_) => {
                        // Data not encrypted, return as-is (for backwards compatibility)
                        Ok(data.to_vec())
                    }
                }
            } else {
                // No encryption configured
                Ok(data.to_vec())
            }
        } else {
            Ok(data.to_vec())
        }
    }

    /// Increment and return the new revision
    fn next_revision(&self) -> StorageResult<i64> {
        let rev = self.current_revision.fetch_add(1, Ordering::SeqCst) + 1;
        self.revisions.insert(b"current", &rev.to_be_bytes())?;
        Ok(rev)
    }

    /// Store a value in history
    fn store_history(&self, key: &str, value: &[u8], revision: i64) -> StorageResult<()> {
        let history_key = format!("{key}@{revision}");
        self.history.insert(history_key.as_bytes(), value)?;
        Ok(())
    }

    /// Get value from history at specific revision
    pub fn get_at_revision(&self, key: &str, revision: i64) -> StorageResult<Option<Vec<u8>>> {
        // Find the highest revision <= requested revision
        let prefix = format!("{key}@");
        let mut result: Option<(i64, Vec<u8>)> = None;

        for item in self.history.scan_prefix(prefix.as_bytes()) {
            let (k, v) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if let Some(rev_str) = key_str.strip_prefix(&prefix) {
                if let Ok(rev) = rev_str.parse::<i64>() {
                    if rev <= revision {
                        match &result {
                            None => result = Some((rev, v.to_vec())),
                            Some((current_rev, _)) if rev > *current_rev => {
                                result = Some((rev, v.to_vec()));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(result.map(|(_, v)| v))
    }

    /// Flush to disk
    pub fn flush(&self) -> StorageResult<()> {
        self.db.flush()?;
        Ok(())
    }
}

#[async_trait]
impl Storage for SledBackend {
    async fn get(&self, key: &str) -> StorageResult<Option<Vec<u8>>> {
        match self.data.get(key.as_bytes())? {
            Some(v) => {
                let decrypted = self.maybe_decrypt(key, &v)?;
                Ok(Some(decrypted))
            }
            None => Ok(None),
        }
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> StorageResult<i64> {
        let revision = self.next_revision()?;

        // Encrypt if needed
        let encrypted_value = self.maybe_encrypt(key, &value)?;

        // Get previous value for watch event
        let prev_value = self.data.get(key.as_bytes())?;

        // Store in data tree (encrypted if secret)
        self.data.insert(key.as_bytes(), encrypted_value.as_slice())?;

        // Store in history (encrypted if secret)
        self.store_history(key, &encrypted_value, revision)?;

        // Store the revision for this key
        let rev_key = format!("rev:{key}");
        self.revisions
            .insert(rev_key.as_bytes(), &revision.to_be_bytes())?;

        // Emit watch event
        let event = match prev_value {
            Some(prev) => WatchEvent::modified(key.to_string(), value, prev.to_vec(), revision),
            None => WatchEvent::added(key.to_string(), value, revision),
        };
        self.watcher.send(event);

        Ok(revision)
    }

    async fn delete(&self, key: &str) -> StorageResult<bool> {
        if let Some(prev_value) = self.data.remove(key.as_bytes())? {
            let revision = self.next_revision()?;

            // Remove from revisions
            let rev_key = format!("rev:{key}");
            self.revisions.remove(rev_key.as_bytes())?;

            // Emit watch event
            self.watcher.send(WatchEvent::deleted(
                key.to_string(),
                prev_value.to_vec(),
                revision,
            ));

            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list(&self, prefix: &str) -> StorageResult<Vec<(String, Vec<u8>)>> {
        let mut results = Vec::new();
        for item in self.data.scan_prefix(prefix.as_bytes()) {
            let (k, v) = item?;
            let key = String::from_utf8_lossy(&k).to_string();
            results.push((key, v.to_vec()));
        }
        Ok(results)
    }

    async fn watch(&self, prefix: &str, _revision: i64) -> StorageResult<Watcher> {
        Ok(self.watcher.subscribe(prefix))
    }

    async fn revision(&self) -> StorageResult<i64> {
        Ok(self.current_revision.load(Ordering::SeqCst))
    }

    async fn compact(&self, revision: i64) -> StorageResult<()> {
        // Remove history entries older than the given revision
        let mut to_remove = Vec::new();

        for item in self.history.iter() {
            let (k, _) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if let Some(rev_pos) = key_str.rfind('@') {
                if let Ok(rev) = key_str[rev_pos + 1..].parse::<i64>() {
                    if rev < revision {
                        to_remove.push(k);
                    }
                }
            }
        }

        for key in to_remove {
            self.history.remove(key)?;
        }

        Ok(())
    }
}

/// Type-safe wrapper for storing Kubernetes resources
pub struct ResourceStore<R> {
    backend: Arc<SledBackend>,
    _phantom: std::marker::PhantomData<R>,
}

impl<R: k1s_types::Resource> ResourceStore<R> {
    pub fn new(backend: Arc<SledBackend>) -> Self {
        Self {
            backend,
            _phantom: std::marker::PhantomData,
        }
    }

    fn key(&self, namespace: Option<&str>, name: &str) -> String {
        match (k1s_types::ResourceScope::Namespaced == R::SCOPE, namespace) {
            (true, Some(ns)) => format!("/{}/{}/{}/{}", R::API_VERSION, R::PLURAL, ns, name),
            (true, None) => format!("/{}/{}/default/{}", R::API_VERSION, R::PLURAL, name),
            (false, _) => format!("/{}/{}/{}", R::API_VERSION, R::PLURAL, name),
        }
    }

    fn prefix(&self, namespace: Option<&str>) -> String {
        match (k1s_types::ResourceScope::Namespaced == R::SCOPE, namespace) {
            (true, Some(ns)) => format!("/{}/{}/{}/", R::API_VERSION, R::PLURAL, ns),
            _ => format!("/{}/{}/", R::API_VERSION, R::PLURAL),
        }
    }

    pub async fn get(&self, namespace: Option<&str>, name: &str) -> StorageResult<Option<R>> {
        let key = self.key(namespace, name);
        match self.backend.get(&key).await? {
            Some(data) => {
                let resource: R = serde_json::from_slice(&data)?;
                Ok(Some(resource))
            }
            None => Ok(None),
        }
    }

    pub async fn list(&self, namespace: Option<&str>) -> StorageResult<Vec<R>> {
        self.list_with_selector(namespace, None).await
    }

    pub async fn list_with_selector(
        &self,
        namespace: Option<&str>,
        selector: Option<&k1s_types::ParsedLabelSelector>,
    ) -> StorageResult<Vec<R>> {
        let prefix = self.prefix(namespace);
        let items = self.backend.list(&prefix).await?;

        let mut resources = Vec::new();
        for (_, data) in items {
            let resource: R = serde_json::from_slice(&data)?;

            // Apply label selector if provided
            if let Some(sel) = selector {
                if !sel.matches(&resource.metadata().labels) {
                    continue;
                }
            }

            resources.push(resource);
        }
        Ok(resources)
    }

    pub async fn create(&self, mut resource: R) -> StorageResult<R> {
        let key = resource.storage_key();

        // Check if already exists
        if self.backend.get(&key).await?.is_some() {
            return Err(StorageError::AlreadyExists(key));
        }

        // Set metadata
        let meta = resource.metadata_mut();
        if meta.uid.is_empty() {
            meta.uid = uuid::Uuid::new_v4().to_string();
        }
        meta.creation_timestamp = Some(chrono::Utc::now());
        meta.resource_version = String::new(); // Will be set after put

        // Store
        let data = serde_json::to_vec(&resource)?;
        let revision = self.backend.put(&key, data).await?;

        // Update resource version
        resource.metadata_mut().resource_version = revision.to_string();

        Ok(resource)
    }

    pub async fn update(&self, mut resource: R) -> StorageResult<R> {
        let key = resource.storage_key();

        // Check if exists
        let existing = self.backend.get(&key).await?;
        if existing.is_none() {
            return Err(StorageError::NotFound(key));
        }

        // Increment generation if spec changed
        let meta = resource.metadata_mut();
        meta.generation = Some(meta.generation.unwrap_or(0) + 1);

        // Store
        let data = serde_json::to_vec(&resource)?;
        let revision = self.backend.put(&key, data).await?;

        // Update resource version
        resource.metadata_mut().resource_version = revision.to_string();

        Ok(resource)
    }

    pub async fn delete(&self, namespace: Option<&str>, name: &str) -> StorageResult<bool> {
        let key = self.key(namespace, name);
        self.backend.delete(&key).await
    }

    pub async fn watch(&self, namespace: Option<&str>) -> StorageResult<Watcher> {
        let prefix = self.prefix(namespace);
        self.backend.watch(&prefix, 0).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_operations() {
        let backend = SledBackend::in_memory().unwrap();

        // Put
        let rev = backend.put("/test/key1", b"value1".to_vec()).await.unwrap();
        assert!(rev > 0);

        // Get
        let value = backend.get("/test/key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Update
        let rev2 = backend.put("/test/key1", b"value2".to_vec()).await.unwrap();
        assert!(rev2 > rev);

        // List
        backend.put("/test/key2", b"value3".to_vec()).await.unwrap();
        let items = backend.list("/test/").await.unwrap();
        assert_eq!(items.len(), 2);

        // Delete
        let deleted = backend.delete("/test/key1").await.unwrap();
        assert!(deleted);

        let value = backend.get("/test/key1").await.unwrap();
        assert!(value.is_none());
    }
}
