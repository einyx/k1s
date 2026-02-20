//! Node agent with container runtime support
//!
//! The kubelet watches for pods assigned to this node and ensures
//! containers are running according to the pod spec.

pub mod pod;
pub mod runtime;
pub mod secrets;

use std::sync::Arc;
use std::time::Duration;

use ipnetwork::Ipv4Network;
use k1s_networking::cni::BridgeConfig;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::Pod;
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

pub use runtime::{ContainerRuntime, RuntimeConfig};

use pod::PodManager;

#[derive(Error, Debug)]
pub enum KubeletError {
    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Storage error: {0}")]
    Storage(#[from] k1s_storage::StorageError),

    #[error("Pod error: {0}")]
    Pod(String),
}

pub type KubeletResult<T> = Result<T, KubeletError>;

/// Kubelet configuration
#[derive(Debug, Clone)]
pub struct KubeletConfig {
    pub node_name: String,
    pub pod_cidr: String,
    pub cluster_dns: String,
    pub cluster_domain: String,
    pub runtime: RuntimeConfig,
    pub sync_interval: Duration,
    pub heartbeat_interval: Duration,
}

impl Default for KubeletConfig {
    fn default() -> Self {
        Self {
            node_name: "k1s-node".to_string(),
            pod_cidr: "10.42.0.0/24".to_string(),
            cluster_dns: "10.43.0.10".to_string(),
            cluster_domain: "cluster.local".to_string(),
            runtime: RuntimeConfig::default(),
            sync_interval: Duration::from_secs(5),
            heartbeat_interval: Duration::from_secs(10),
        }
    }
}

/// The kubelet node agent
pub struct Kubelet {
    config: KubeletConfig,
    storage: Arc<SledBackend>,
    runtime: Arc<dyn ContainerRuntime>,
    pod_manager: PodManager,
}

impl Kubelet {
    pub async fn new(
        config: KubeletConfig,
        storage: Arc<SledBackend>,
    ) -> KubeletResult<Self> {
        let runtime: Arc<dyn ContainerRuntime> = match config.runtime.runtime_type.as_str() {
            "containerd" => {
                Arc::new(runtime::containerd::ContainerdRuntime::new(&config.runtime).await?)
            }
            _ => Arc::new(runtime::docker::DockerRuntime::new(&config.runtime).await?),
        };

        // Configure CNI from pod_cidr
        let cni_config = Self::build_cni_config(&config.pod_cidr);
        let pod_manager = PodManager::with_cni_config(runtime.clone(), storage.clone(), cni_config);

        // Initialize CNI (creates Docker network on macOS, bridge on Linux)
        if let Err(e) = pod_manager.init_cni().await {
            error!("Failed to initialize CNI: {}", e);
        }

        info!(
            "Kubelet initialized with {} runtime, pod CIDR: {}",
            runtime.name(),
            config.pod_cidr
        );

        Ok(Self {
            config,
            storage,
            runtime,
            pod_manager,
        })
    }

    /// Build CNI configuration from pod CIDR
    fn build_cni_config(pod_cidr: &str) -> BridgeConfig {
        let subnet: Ipv4Network = pod_cidr.parse().unwrap_or_else(|_| {
            "10.42.0.0/24".parse().unwrap()
        });

        // Gateway is typically the first usable IP in the subnet
        let gateway = {
            let network_addr = u32::from(subnet.ip());
            std::net::Ipv4Addr::from(network_addr + 1)
        };

        BridgeConfig {
            bridge_name: "k1s0".to_string(),
            subnet,
            gateway,
            mtu: 1500,
            masquerade: true,
        }
    }

    /// Run the kubelet main loop
    pub async fn run(&self) -> KubeletResult<()> {
        info!("Starting kubelet for node {}", self.config.node_name);

        let mut sync_ticker = interval(self.config.sync_interval);
        let mut heartbeat_ticker = interval(self.config.heartbeat_interval);

        loop {
            tokio::select! {
                _ = sync_ticker.tick() => {
                    if let Err(e) = self.sync_pods().await {
                        error!("Pod sync failed: {}", e);
                    }
                }
                _ = heartbeat_ticker.tick() => {
                    if let Err(e) = self.heartbeat().await {
                        error!("Heartbeat failed: {}", e);
                    }
                }
            }
        }
    }

    /// Sync all pods assigned to this node
    async fn sync_pods(&self) -> KubeletResult<()> {
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        // List all pods
        let pods = pod_store.list(None).await?;

        for pod in pods {
            // Check if pod is assigned to this node
            let node_name = pod
                .spec
                .as_ref()
                .and_then(|s| s.node_name.as_ref());

            if node_name != Some(&self.config.node_name) {
                continue;
            }

            // Check if pod is being deleted
            if pod.metadata.deletion_timestamp.is_some() {
                debug!("Deleting pod {}", pod.metadata.name);
                if let Err(e) = self.delete_pod(&pod).await {
                    error!("Failed to delete pod {}: {}", pod.metadata.name, e);
                }
                continue;
            }

            // Sync the pod
            if let Err(e) = self.sync_pod(&pod).await {
                error!("Failed to sync pod {}: {}", pod.metadata.name, e);
            }
        }

        Ok(())
    }

    /// Sync a single pod
    async fn sync_pod(&self, pod: &Pod) -> KubeletResult<()> {
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        // Clone pod for mutation
        let mut pod = pod.clone();

        // Sync pod containers
        self.pod_manager.sync_pod(&mut pod).await?;

        // Update pod status
        pod_store.update(pod).await?;

        Ok(())
    }

    /// Delete a pod and its containers
    async fn delete_pod(&self, pod: &Pod) -> KubeletResult<()> {
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        // Delete containers
        self.pod_manager.delete_pod(pod).await?;

        // Remove pod from storage
        pod_store
            .delete(pod.metadata.namespace.as_deref(), &pod.metadata.name)
            .await?;

        info!("Deleted pod {}/{}", pod.metadata.effective_namespace(), pod.metadata.name);

        Ok(())
    }

    /// Send heartbeat to update node status
    async fn heartbeat(&self) -> KubeletResult<()> {
        use k1s_types::{Node, NodeCondition, NodeConditionType};

        let store = ResourceStore::<Node>::new(self.storage.clone());

        if let Some(mut node) = store.get(None, &self.config.node_name).await? {
            if let Some(status) = &mut node.status {
                // Update Ready condition
                for condition in &mut status.conditions {
                    if matches!(condition.r#type, NodeConditionType::Ready) {
                        condition.last_heartbeat_time = Some(chrono::Utc::now());
                    }
                }
            }

            store.update(node).await?;
            debug!("Sent heartbeat for node {}", self.config.node_name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kubelet_config_default() {
        let config = KubeletConfig::default();
        assert_eq!(config.node_name, "k1s-node");
        assert_eq!(config.sync_interval, Duration::from_secs(5));
    }
}
