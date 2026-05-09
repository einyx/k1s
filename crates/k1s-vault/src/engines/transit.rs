//! Transit engine: Encryption-as-a-Service
//!
//! Provides encrypt/decrypt operations without storing data

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use k1s_storage::{SledBackend, Storage};
use crate::audit::{AuditLogger, AuditOperation, AuthInfo};
use crate::error::{VaultError, VaultResult};

const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitKey {
    pub name: String,
    pub version: u32,
    pub key_data: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub min_decryption_version: u32,
    pub deletion_allowed: bool,
}

pub struct TransitEngine {
    storage: Arc<SledBackend>,
    audit: Arc<AuditLogger>,
    key_cache: Arc<RwLock<HashMap<String, Aes256Gcm>>>,
}

impl TransitEngine {
    pub fn new(storage: Arc<SledBackend>, audit: Arc<AuditLogger>) -> VaultResult<Self> {
        Ok(Self {
            storage,
            audit,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new encryption key
    pub async fn create_key(&self, name: &str) -> VaultResult<()> {
        // Check if key exists
        let key_path = format!("transit/keys/{name}");
        if self.storage.get(&key_path).await?.is_some() {
            return Err(VaultError::InvalidInput(format!("Key {name} already exists")));
        }

        // Generate new key
        let mut key_data = vec![0u8; KEY_SIZE];
        OsRng.fill_bytes(&mut key_data);

        let transit_key = TransitKey {
            name: name.to_string(),
            version: 1,
            key_data,
            created_at: chrono::Utc::now(),
            min_decryption_version: 1,
            deletion_allowed: false,
        };

        // Store key
        let data = serde_json::to_vec(&transit_key)
            .map_err(|e| VaultError::Internal(e.to_string()))?;
        self.storage.put(&key_path, data).await?;

        tracing::info!("Created transit key: {}", name);
        Ok(())
    }

    /// Load a key from storage
    async fn load_key(&self, name: &str) -> VaultResult<Aes256Gcm> {
        // Check cache first
        if let Some(cipher) = self.key_cache.read().unwrap().get(name) {
            return Ok(cipher.clone());
        }

        // Load from storage
        let key_path = format!("transit/keys/{name}");
        let data = self.storage.get(&key_path).await?
            .ok_or_else(|| VaultError::KeyNotFound(name.to_string()))?;

        let transit_key: TransitKey = serde_json::from_slice(&data)
            .map_err(|e| VaultError::Internal(e.to_string()))?;

        let key: &Key<Aes256Gcm> = transit_key.key_data.as_slice().into();
        let cipher = Aes256Gcm::new(key);

        // Cache it
        self.key_cache.write().unwrap().insert(name.to_string(), cipher.clone());

        Ok(cipher)
    }

    /// Encrypt data
    pub async fn encrypt(
        &self,
        key_name: &str,
        plaintext: &[u8],
        auth: AuthInfo,
    ) -> VaultResult<String> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::TransitEncrypt,
            auth,
            format!("/transit/encrypt/{key_name}"),
            None,
        );

        let result: VaultResult<String> = async {
            let cipher = self.load_key(key_name).await?;

            // Generate random nonce
            let mut nonce_bytes = [0u8; NONCE_SIZE];
            OsRng.fill_bytes(&mut nonce_bytes);
            let nonce = Nonce::from_slice(&nonce_bytes);

            // Encrypt
            let ciphertext = cipher
                .encrypt(nonce, plaintext)
                .map_err(|e| VaultError::EncryptionFailed(e.to_string()))?;

            // Format: vault:v1:base64(nonce || ciphertext)
            let mut result = nonce_bytes.to_vec();
            result.extend_from_slice(&ciphertext);
            Ok(format!("vault:v1:{}", BASE64.encode(&result)))
        }.await;

        // Audit response
        match &result {
            Ok(_) => {
                let entry = self.audit.response(entry, "success".to_string(), None);
                self.audit.log(entry).await;
            }
            Err(e) => {
                let entry = self.audit.response(entry, "error".to_string(), Some(e.to_string()));
                self.audit.log(entry).await;
            }
        }

        result
    }

    /// Decrypt data
    pub async fn decrypt(
        &self,
        key_name: &str,
        ciphertext: &str,
        auth: AuthInfo,
    ) -> VaultResult<Vec<u8>> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::TransitDecrypt,
            auth,
            format!("/transit/decrypt/{key_name}"),
            None,
        );

        let result: VaultResult<Vec<u8>> = async {
            // Parse vault:v1:base64 format
            let parts: Vec<&str> = ciphertext.split(':').collect();
            if parts.len() != 3 || parts[0] != "vault" || parts[1] != "v1" {
                return Err(VaultError::InvalidInput("Invalid ciphertext format".to_string()));
            }

            let data = BASE64.decode(parts[2])
                .map_err(|e| VaultError::InvalidInput(format!("Invalid base64: {e}")))?;

            if data.len() < NONCE_SIZE {
                return Err(VaultError::InvalidInput("Ciphertext too short".to_string()));
            }

            let (nonce_bytes, ciphertext_bytes) = data.split_at(NONCE_SIZE);
            let nonce = Nonce::from_slice(nonce_bytes);

            let cipher = self.load_key(key_name).await?;

            cipher
                .decrypt(nonce, ciphertext_bytes)
                .map_err(|e| VaultError::DecryptionFailed(e.to_string()))
        }.await;

        // Audit response
        match &result {
            Ok(_) => {
                let entry = self.audit.response(entry, "success".to_string(), None);
                self.audit.log(entry).await;
            }
            Err(e) => {
                let entry = self.audit.response(entry, "error".to_string(), Some(e.to_string()));
                self.audit.log(entry).await;
            }
        }

        result
    }

    /// List all transit keys
    pub async fn list_keys(&self) -> VaultResult<Vec<String>> {
        let entries = self.storage.list("transit/keys/").await?;
        let keys: Vec<String> = entries
            .into_iter()
            .filter_map(|(path, _)| {
                path.strip_prefix("transit/keys/").map(|s| s.to_string())
            })
            .collect();
        Ok(keys)
    }

    /// Delete a key
    pub async fn delete_key(&self, name: &str) -> VaultResult<()> {
        let key_path = format!("transit/keys/{name}");
        self.storage.delete(&key_path).await?;
        self.key_cache.write().unwrap().remove(name);
        tracing::info!("Deleted transit key: {}", name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transit_encrypt_decrypt() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let audit = Arc::new(AuditLogger::new(storage.clone()));
        let transit = TransitEngine::new(storage, audit).unwrap();

        transit.create_key("test-key").await.unwrap();

        let auth = AuthInfo {
            user: "test".to_string(),
            namespace: None,
            source_ip: None,
        };

        let plaintext = b"secret data";
        let ciphertext = transit.encrypt("test-key", plaintext, auth.clone()).await.unwrap();
        let decrypted = transit.decrypt("test-key", &ciphertext, auth).await.unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }
}
