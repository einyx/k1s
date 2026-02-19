//! Application state shared across handlers

use std::sync::Arc;

use k1s_storage::SledBackend;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<SledBackend>,
    pub node_name: String,
    pub cluster_domain: String,
}

impl AppState {
    pub fn new(storage: Arc<SledBackend>, node_name: String) -> Self {
        Self {
            storage,
            node_name,
            cluster_domain: "cluster.local".to_string(),
        }
    }
}
