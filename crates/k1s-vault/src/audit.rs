//! Audit logging for all vault operations

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use k1s_storage::{SledBackend, Storage};
use tracing::error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditType {
    Request,
    Response,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOperation {
    TransitEncrypt,
    TransitDecrypt,
    KvRead,
    KvWrite,
    KvDelete,
    PkiIssue,
    PkiSign,
    PkiRevoke,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    pub user: String,
    pub namespace: Option<String>,
    pub source_ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub audit_type: AuditType,
    pub operation: AuditOperation,
    pub auth: AuthInfo,
    pub path: String,
    pub request_data: Option<serde_json::Value>,
    pub response_status: Option<String>,
    pub error: Option<String>,
}

pub struct AuditLogger {
    storage: Arc<SledBackend>,
}

impl AuditLogger {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Log an audit entry
    pub async fn log(&self, entry: AuditEntry) {
        let key = format!("audit/log/{}/{}", entry.timestamp.timestamp_millis(), entry.id);

        match serde_json::to_vec(&entry) {
            Ok(data) => {
                if let Err(e) = self.storage.put(&key, data).await {
                    error!("Failed to write audit log: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to serialize audit entry: {}", e);
            }
        }
    }

    /// Create an audit entry for a request
    pub fn request(
        &self,
        operation: AuditOperation,
        auth: AuthInfo,
        path: String,
        request_data: Option<serde_json::Value>,
    ) -> AuditEntry {
        AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            audit_type: AuditType::Request,
            operation,
            auth,
            path,
            request_data,
            response_status: None,
            error: None,
        }
    }

    /// Create an audit entry for a response
    pub fn response(
        &self,
        mut entry: AuditEntry,
        status: String,
        error: Option<String>,
    ) -> AuditEntry {
        entry.audit_type = AuditType::Response;
        entry.response_status = Some(status);
        entry.error = error;
        entry.timestamp = Utc::now();
        entry
    }

    /// Query audit logs
    pub async fn query(
        &self,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<Vec<AuditEntry>, k1s_storage::StorageError> {
        let entries = self.storage.list("audit/log/").await?;

        let mut logs: Vec<AuditEntry> = entries
            .into_iter()
            .filter_map(|(_, data)| serde_json::from_slice(&data).ok())
            .collect();

        // Filter by time range
        if let Some(start) = start_time {
            logs.retain(|e| e.timestamp >= start);
        }
        if let Some(end) = end_time {
            logs.retain(|e| e.timestamp <= end);
        }

        logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(logs)
    }
}
