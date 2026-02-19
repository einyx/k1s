//! Namespace lifecycle controller

use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{Namespace, NamespacePhase};
use tracing::info;

use crate::{Controller, ControllerResult};

pub struct NamespaceController {
    storage: Arc<SledBackend>,
}

impl NamespaceController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl Controller for NamespaceController {
    fn name(&self) -> &str {
        "namespace"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let store = ResourceStore::<Namespace>::new(self.storage.clone());
        let namespaces = store.list(None).await?;

        for ns in namespaces {
            // Check for terminating namespaces
            if let Some(deletion_ts) = &ns.metadata.deletion_timestamp {
                info!("Namespace {} is terminating", ns.metadata.name);

                // Check if finalizers are empty
                if ns.metadata.finalizers.is_empty() {
                    // Delete the namespace
                    store.delete(None, &ns.metadata.name).await?;
                    info!("Deleted namespace {}", ns.metadata.name);
                }
            }

            // Ensure status is set
            if ns.status.is_none() {
                let mut updated = ns.clone();
                updated.status = Some(k1s_types::NamespaceStatus {
                    phase: Some(NamespacePhase::Active),
                    conditions: vec![],
                });
                store.update(updated).await?;
            }
        }

        Ok(())
    }
}
