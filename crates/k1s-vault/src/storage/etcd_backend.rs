//! etcd storage backend for vault (k3s compatible)

use async_trait::async_trait;
use etcd_client::{Client, GetOptions, PutOptions, DeleteOptions, WatchOptions};
use futures::StreamExt;

use crate::error::{VaultError, VaultResult};
use super::{VaultStorage, WatchEvent};

pub struct EtcdStorage {
    client: Client,
    prefix: String,
}

impl EtcdStorage {
    /// Create new etcd storage backend
    pub async fn new(endpoints: Vec<String>, prefix: Option<String>) -> VaultResult<Self> {
        let client = Client::connect(endpoints, None).await
            .map_err(|e| VaultError::Internal(format!("Failed to connect to etcd: {}", e)))?;

        Ok(Self {
            client,
            prefix: prefix.unwrap_or_else(|| "/k1s/vault".to_string()),
        })
    }

    /// Create from k3s configuration
    pub async fn from_k3s_config(config_path: &str) -> VaultResult<Self> {
        // Read k3s config to get etcd endpoints
        // k3s typically uses: /var/lib/rancher/k3s/server/db/etcd
        let endpoints = vec![
            "http://127.0.0.1:2379".to_string(), // k3s default
        ];

        Self::new(endpoints, Some("/k1s/vault".to_string())).await
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}/{}", self.prefix, key)
    }
}

#[async_trait]
impl VaultStorage for EtcdStorage {
    async fn get(&self, key: &str) -> VaultResult<Option<Vec<u8>>> {
        let full_key = self.full_key(key);

        let resp = self.client.get(full_key, None).await
            .map_err(|e| VaultError::Internal(format!("etcd get failed: {}", e)))?;

        Ok(resp.kvs().first().map(|kv| kv.value().to_vec()))
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> VaultResult<()> {
        let full_key = self.full_key(key);

        self.client.put(full_key, value, None).await
            .map_err(|e| VaultError::Internal(format!("etcd put failed: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> VaultResult<()> {
        let full_key = self.full_key(key);

        self.client.delete(full_key, None).await
            .map_err(|e| VaultError::Internal(format!("etcd delete failed: {}", e)))?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> VaultResult<Vec<(String, Vec<u8>)>> {
        let full_prefix = self.full_key(prefix);

        let mut opts = GetOptions::new();
        opts = opts.with_prefix();

        let resp = self.client.get(full_prefix.clone(), Some(opts)).await
            .map_err(|e| VaultError::Internal(format!("etcd list failed: {}", e)))?;

        let results = resp.kvs()
            .iter()
            .filter_map(|kv| {
                let key = String::from_utf8(kv.key().to_vec()).ok()?;
                let key = key.strip_prefix(&format!("{}/", self.prefix))?;
                Some((key.to_string(), kv.value().to_vec()))
            })
            .collect();

        Ok(results)
    }

    async fn watch(&self, prefix: &str) -> VaultResult<Box<dyn futures::Stream<Item = WatchEvent> + Send + Unpin>> {
        let full_prefix = self.full_key(prefix);

        let mut opts = WatchOptions::new();
        opts = opts.with_prefix();

        let (mut watcher, mut stream) = self.client.watch(full_prefix.clone(), Some(opts)).await
            .map_err(|e| VaultError::Internal(format!("etcd watch failed: {}", e)))?;

        // Create a stream that converts etcd events to WatchEvents
        let prefix_len = format!("{}/", self.prefix).len();
        let event_stream = async_stream::stream! {
            while let Some(resp) = stream.next().await {
                if let Ok(resp) = resp {
                    for event in resp.events() {
                        let kv = event.kv().unwrap();
                        let key = String::from_utf8_lossy(kv.key()).to_string();
                        let key = key[prefix_len..].to_string();

                        match event.event_type() {
                            etcd_client::EventType::Put => {
                                yield WatchEvent::Put {
                                    key,
                                    value: kv.value().to_vec(),
                                };
                            }
                            etcd_client::EventType::Delete => {
                                yield WatchEvent::Delete { key };
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::new(Box::pin(event_stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires etcd running
    async fn test_etcd_basic_operations() {
        let storage = EtcdStorage::new(
            vec!["http://127.0.0.1:2379".to_string()],
            Some("/test/vault".to_string())
        ).await.unwrap();

        // Put
        storage.put("test/key", b"test value".to_vec()).await.unwrap();

        // Get
        let value = storage.get("test/key").await.unwrap();
        assert_eq!(value, Some(b"test value".to_vec()));

        // List
        let entries = storage.list("test").await.unwrap();
        assert_eq!(entries.len(), 1);

        // Delete
        storage.delete("test/key").await.unwrap();
        let value = storage.get("test/key").await.unwrap();
        assert_eq!(value, None);
    }
}
