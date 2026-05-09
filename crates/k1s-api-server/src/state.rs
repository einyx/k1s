//! Application state shared across handlers

use std::sync::Arc;

use k1s_storage::SledBackend;
use k1s_vault::Vault;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<SledBackend>,
    pub node_name: String,
    pub cluster_domain: String,
    pub vault: Arc<Vault>,
}

impl AppState {
    pub fn new(storage: Arc<SledBackend>, node_name: String) -> Self {
        let vault = Arc::new(
            Vault::new(storage.clone())
                .expect("Failed to initialize vault")
        );

        Self {
            storage,
            node_name,
            cluster_domain: "cluster.local".to_string(),
            vault,
        }
    }
}
