//! KV engine: Versioned secret storage
//!
//! Provides key-value storage with versioning, metadata, and soft deletes

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use k1s_storage::{SledBackend, Storage};
use crate::audit::{AuditLogger, AuditOperation, AuthInfo};
use crate::error::{VaultError, VaultResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub created_time: DateTime<Utc>,
    pub updated_time: DateTime<Utc>,
    pub current_version: u32,
    pub oldest_version: u32,
    pub max_versions: u32,
    pub cas_required: bool,
    pub delete_version_after: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretVersion {
    pub version: u32,
    pub data: HashMap<String, String>,
    pub created_time: DateTime<Utc>,
    pub deletion_time: Option<DateTime<Utc>>,
    pub destroyed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretData {
    pub data: HashMap<String, String>,
    pub metadata: SecretVersionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretVersionMetadata {
    pub version: u32,
    pub created_time: DateTime<Utc>,
    pub deletion_time: Option<DateTime<Utc>>,
    pub destroyed: bool,
}

pub struct KvEngine {
    storage: Arc<SledBackend>,
    audit: Arc<AuditLogger>,
}

impl KvEngine {
    pub fn new(storage: Arc<SledBackend>, audit: Arc<AuditLogger>) -> Self {
        Self { storage, audit }
    }

    /// Read a secret at a specific version
    pub async fn read(
        &self,
        path: &str,
        version: Option<u32>,
        auth: AuthInfo,
    ) -> VaultResult<SecretData> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::KvRead,
            auth,
            format!("/kv/data/{}", path),
            None,
        );

        let result = async {
            let metadata = self.get_metadata(path).await?;
            let version = version.unwrap_or(metadata.current_version);

            // Load version
            let version_path = format!("kv/data/{}/v{}", path, version);
            let data = self.storage.get(&version_path).await?
                .ok_or_else(|| VaultError::SecretNotFound(format!("{} version {}", path, version)))?;

            let secret_version: SecretVersion = serde_json::from_slice(&data)
                .map_err(|e| VaultError::Internal(e.to_string()))?;

            // Check if deleted or destroyed
            if secret_version.destroyed {
                return Err(VaultError::SecretNotFound(format!("{} (destroyed)", path)));
            }
            if secret_version.deletion_time.is_some() {
                return Err(VaultError::SecretNotFound(format!("{} (deleted)", path)));
            }

            Ok(SecretData {
                data: secret_version.data,
                metadata: SecretVersionMetadata {
                    version: secret_version.version,
                    created_time: secret_version.created_time,
                    deletion_time: secret_version.deletion_time,
                    destroyed: secret_version.destroyed,
                },
            })
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

    /// Write a new version of a secret
    pub async fn write(
        &self,
        path: &str,
        data: HashMap<String, String>,
        cas: Option<u32>,
        auth: AuthInfo,
    ) -> VaultResult<SecretVersionMetadata> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::KvWrite,
            auth,
            format!("/kv/data/{}", path),
            Some(serde_json::json!({"keys": data.keys().collect::<Vec<_>>()})),
        );

        let result = async {
            // Get or create metadata
            let mut metadata = self.get_metadata(path).await
                .unwrap_or_else(|_| SecretMetadata {
                    created_time: Utc::now(),
                    updated_time: Utc::now(),
                    current_version: 0,
                    oldest_version: 1,
                    max_versions: 10,
                    cas_required: false,
                    delete_version_after: None,
                });

            // Check CAS
            if let Some(cas_version) = cas {
                if cas_version != metadata.current_version {
                    return Err(VaultError::InvalidInput(
                        format!("CAS mismatch: expected {}, got {}", metadata.current_version, cas_version)
                    ));
                }
            } else if metadata.cas_required {
                return Err(VaultError::InvalidInput("CAS value required".to_string()));
            }

            // Create new version
            let new_version = metadata.current_version + 1;
            let secret_version = SecretVersion {
                version: new_version,
                data,
                created_time: Utc::now(),
                deletion_time: None,
                destroyed: false,
            };

            // Store version
            let version_path = format!("kv/data/{}/v{}", path, new_version);
            let version_data = serde_json::to_vec(&secret_version)
                .map_err(|e| VaultError::Internal(e.to_string()))?;
            self.storage.put(&version_path, version_data).await?;

            // Update metadata
            metadata.current_version = new_version;
            metadata.updated_time = Utc::now();

            // Cleanup old versions if needed
            if new_version > metadata.max_versions {
                let old_version = new_version - metadata.max_versions;
                let old_path = format!("kv/data/{}/v{}", path, old_version);
                let _ = self.storage.delete(&old_path).await;
                metadata.oldest_version = old_version + 1;
            }

            // Store metadata
            let metadata_path = format!("kv/metadata/{}", path);
            let metadata_data = serde_json::to_vec(&metadata)
                .map_err(|e| VaultError::Internal(e.to_string()))?;
            self.storage.put(&metadata_path, metadata_data).await?;

            Ok(SecretVersionMetadata {
                version: new_version,
                created_time: secret_version.created_time,
                deletion_time: None,
                destroyed: false,
            })
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

    /// Delete a secret (soft delete)
    pub async fn delete(
        &self,
        path: &str,
        versions: Option<Vec<u32>>,
        auth: AuthInfo,
    ) -> VaultResult<()> {
        // Audit log
        let entry = self.audit.request(
            AuditOperation::KvDelete,
            auth,
            format!("/kv/data/{}", path),
            versions.as_ref().map(|v| serde_json::json!({"versions": v})),
        );

        let result: VaultResult<()> = async {
            let metadata = self.get_metadata(path).await?;
            let versions = versions.unwrap_or_else(|| vec![metadata.current_version]);

            for version in versions {
                let version_path = format!("kv/data/{}/v{}", path, version);
                if let Some(data) = self.storage.get(&version_path).await? {
                    let mut secret_version: SecretVersion = serde_json::from_slice(&data)
                        .map_err(|e| VaultError::Internal(e.to_string()))?;

                    secret_version.deletion_time = Some(Utc::now());

                    let updated_data = serde_json::to_vec(&secret_version)
                        .map_err(|e| VaultError::Internal(e.to_string()))?;
                    self.storage.put(&version_path, updated_data).await?;
                }
            }

            Ok(())
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

    /// Get secret metadata
    async fn get_metadata(&self, path: &str) -> VaultResult<SecretMetadata> {
        let metadata_path = format!("kv/metadata/{}", path);
        let data = self.storage.get(&metadata_path).await?
            .ok_or_else(|| VaultError::SecretNotFound(path.to_string()))?;

        serde_json::from_slice(&data)
            .map_err(|e| VaultError::Internal(e.to_string()))
    }

    /// List secrets at a path
    pub async fn list(&self, prefix: &str) -> VaultResult<Vec<String>> {
        let search_prefix = format!("kv/metadata/{}", prefix);
        let entries = self.storage.list(&search_prefix).await?;

        let secrets: Vec<String> = entries
            .into_iter()
            .filter_map(|(path, _)| {
                path.strip_prefix("kv/metadata/").map(|s| s.to_string())
            })
            .collect();

        Ok(secrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kv_versioning() {
        let storage = Arc::new(SledBackend::in_memory().unwrap());
        let audit = Arc::new(AuditLogger::new(storage.clone()));
        let kv = KvEngine::new(storage, audit);

        let auth = AuthInfo {
            user: "test".to_string(),
            namespace: None,
            source_ip: None,
        };

        // Write v1
        let mut data = HashMap::new();
        data.insert("password".to_string(), "secret1".to_string());
        kv.write("db/postgres", data.clone(), None, auth.clone()).await.unwrap();

        // Write v2
        data.insert("password".to_string(), "secret2".to_string());
        kv.write("db/postgres", data, None, auth.clone()).await.unwrap();

        // Read current (v2)
        let secret = kv.read("db/postgres", None, auth.clone()).await.unwrap();
        assert_eq!(secret.metadata.version, 2);
        assert_eq!(secret.data.get("password").unwrap(), "secret2");

        // Read v1
        let secret = kv.read("db/postgres", Some(1), auth).await.unwrap();
        assert_eq!(secret.metadata.version, 1);
        assert_eq!(secret.data.get("password").unwrap(), "secret1");
    }
}
