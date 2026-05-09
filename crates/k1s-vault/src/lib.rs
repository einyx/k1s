//! k1s-vault: Embedded Vault-like secret engine
//!
//! Provides:
//! - Transit engine: Encryption-as-a-Service
//! - KV engine: Versioned secret storage
//! - PKI engine: Certificate management
//! - Audit logging: Complete audit trail

pub mod engines;
pub mod error;
pub mod audit;

pub use engines::{
    transit::TransitEngine,
    kv::KvEngine,
    pki::PkiEngine,
};
pub use audit::{AuditLogger, AuditEntry, AuditOperation, AuthInfo};
pub use error::{VaultError, VaultResult};

use std::sync::Arc;
use k1s_storage::SledBackend;

/// Complete vault instance with all engines
pub struct Vault {
    pub transit: TransitEngine,
    pub kv: KvEngine,
    pub pki: PkiEngine,
    pub audit: Arc<AuditLogger>,
}

impl Vault {
    /// Create a new vault instance
    pub fn new(storage: Arc<SledBackend>) -> VaultResult<Self> {
        let audit = Arc::new(AuditLogger::new(storage.clone()));

        Ok(Self {
            transit: TransitEngine::new(storage.clone(), audit.clone())?,
            kv: KvEngine::new(storage.clone(), audit.clone()),
            pki: PkiEngine::new(storage.clone(), audit.clone())?,
            audit,
        })
    }
}
