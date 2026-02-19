//! Container runtime abstractions

pub mod containerd;
pub mod docker;

use std::collections::HashMap;

use async_trait::async_trait;
use k1s_types::Pod;

use crate::KubeletResult;

/// Container runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub runtime_type: String,
    pub socket_path: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            runtime_type: "docker".to_string(),
            socket_path: "/var/run/docker.sock".to_string(),
        }
    }
}

/// Container info
#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Stopped,
    Unknown,
}

/// Image info
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub id: String,
    pub tags: Vec<String>,
    pub size: u64,
}

/// Additional container configuration for secrets/configmaps
#[derive(Debug, Clone, Default)]
pub struct ContainerEnvConfig {
    /// Additional environment variables (KEY=VALUE format)
    pub env_vars: Vec<String>,
    /// Volume mount data: mount_path -> (filename -> content)
    pub volume_data: HashMap<String, HashMap<String, Vec<u8>>>,
}

/// Container runtime trait
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// Runtime name
    fn name(&self) -> &str;

    /// Pull an image
    async fn pull_image(&self, image: &str) -> KubeletResult<()>;

    /// Create a container for a pod
    async fn create_container(
        &self,
        pod: &Pod,
        container_name: &str,
    ) -> KubeletResult<String> {
        self.create_container_with_env(pod, container_name, &ContainerEnvConfig::default())
            .await
    }

    /// Create a container for a pod with additional environment and volume config
    async fn create_container_with_env(
        &self,
        pod: &Pod,
        container_name: &str,
        env_config: &ContainerEnvConfig,
    ) -> KubeletResult<String>;

    /// Start a container
    async fn start_container(&self, container_id: &str) -> KubeletResult<()>;

    /// Stop a container
    async fn stop_container(&self, container_id: &str, timeout: u64) -> KubeletResult<()>;

    /// Remove a container
    async fn remove_container(&self, container_id: &str) -> KubeletResult<()>;

    /// Get container info
    async fn get_container(&self, container_id: &str) -> KubeletResult<Option<ContainerInfo>>;

    /// List containers for a pod
    async fn list_containers(&self, pod_uid: &str) -> KubeletResult<Vec<ContainerInfo>>;

    /// Get container logs
    async fn logs(&self, container_id: &str, tail: Option<u32>) -> KubeletResult<String>;

    /// Execute command in container
    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
    ) -> KubeletResult<(i32, String, String)>;

    /// List images
    async fn list_images(&self) -> KubeletResult<Vec<ImageInfo>>;
}
