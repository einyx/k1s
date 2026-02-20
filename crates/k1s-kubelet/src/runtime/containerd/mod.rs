//! containerd runtime implementation
//!
//! Implements container operations via containerd's gRPC API.
//! Supports the standard containerd socket at /run/containerd/containerd.sock

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use k1s_types::Pod;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use tracing::{debug, info};

use super::{ContainerInfo, ContainerRuntime, ContainerState, ContainerEnvConfig, ImageInfo, RuntimeConfig};
use crate::{KubeletError, KubeletResult};

mod proto;

use proto::containerd::services::containers::v1::{
    containers_client::ContainersClient, Container, CreateContainerRequest,
    DeleteContainerRequest, GetContainerRequest, ListContainersRequest,
};
use proto::containerd::services::images::v1::{images_client::ImagesClient, ListImagesRequest};
use proto::containerd::services::tasks::v1::{
    tasks_client::TasksClient, CreateTaskRequest, DeleteTaskRequest, GetRequest,
    KillRequest, StartRequest,
};
use proto::containerd::services::namespaces::v1::{
    namespaces_client::NamespacesClient, CreateNamespaceRequest, GetNamespaceRequest, Namespace,
};

const DEFAULT_NAMESPACE: &str = "k1s";
const CONTAINER_LABEL_POD_UID: &str = "k1s.pod.uid";
const CONTAINER_LABEL_POD_NAME: &str = "k1s.pod.name";
const CONTAINER_LABEL_POD_NAMESPACE: &str = "k1s.pod.namespace";
const CONTAINER_LABEL_CONTAINER_NAME: &str = "k1s.container.name";

pub struct ContainerdRuntime {
    socket_path: String,
    channel: Channel,
    namespace: String,
}

impl ContainerdRuntime {
    pub async fn new(config: &RuntimeConfig) -> KubeletResult<Self> {
        let socket_path = if config.socket_path.is_empty() {
            "/run/containerd/containerd.sock".to_string()
        } else {
            config.socket_path.clone()
        };

        let channel = connect_unix(&socket_path).await?;

        let runtime = Self {
            socket_path,
            channel,
            namespace: DEFAULT_NAMESPACE.to_string(),
        };

        // Ensure namespace exists
        runtime.ensure_namespace().await?;

        info!("Connected to containerd");
        Ok(runtime)
    }

    async fn ensure_namespace(&self) -> KubeletResult<()> {
        let mut client = NamespacesClient::new(self.channel.clone());

        let request = GetNamespaceRequest {
            name: self.namespace.clone(),
        };

        match client.get(request).await {
            Ok(_) => Ok(()),
            Err(status) if status.code() == tonic::Code::NotFound => {
                let request = CreateNamespaceRequest {
                    namespace: Some(Namespace {
                        name: self.namespace.clone(),
                        labels: HashMap::new(),
                    }),
                };
                client
                    .create(request)
                    .await
                    .map_err(|e| KubeletError::Runtime(format!("Failed to create namespace: {}", e)))?;
                info!("Created containerd namespace: {}", self.namespace);
                Ok(())
            }
            Err(e) => Err(KubeletError::Runtime(format!(
                "Failed to check namespace: {}",
                e
            ))),
        }
    }

    fn with_namespace<T>(&self, mut request: tonic::Request<T>) -> tonic::Request<T> {
        request
            .metadata_mut()
            .insert("containerd-namespace", self.namespace.parse().unwrap());
        request
    }
}

async fn connect_unix(socket_path: &str) -> KubeletResult<Channel> {
    let socket_path = socket_path.to_string();

    let channel = Endpoint::try_from("http://[::]:50051")
        .map_err(|e| KubeletError::Runtime(format!("Invalid endpoint: {}", e)))?
        .connect_with_connector(service_fn(move |_: Uri| {
            let path = socket_path.clone();
            async move { UnixStream::connect(path).await }
        }))
        .await
        .map_err(|e| KubeletError::Runtime(format!("Failed to connect to containerd: {}", e)))?;

    Ok(channel)
}

#[async_trait]
impl ContainerRuntime for ContainerdRuntime {
    fn name(&self) -> &str {
        "containerd"
    }

    async fn pull_image(&self, image: &str) -> KubeletResult<()> {
        info!("Pulling image: {}", image);

        // For containerd, we need to use the transfer service or content service
        // to pull images. For simplicity, we'll shell out to ctr for now.
        // In production, this should use the proper gRPC transfer API.
        let output = tokio::process::Command::new("ctr")
            .args(["-n", &self.namespace, "images", "pull", image])
            .output()
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to pull image: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(KubeletError::Runtime(format!(
                "Failed to pull image: {}",
                stderr
            )));
        }

        info!("Pulled image: {}", image);
        Ok(())
    }

    async fn create_container_with_env(
        &self,
        pod: &Pod,
        container_name: &str,
        env_config: &ContainerEnvConfig,
    ) -> KubeletResult<String> {
        let spec = pod
            .spec
            .as_ref()
            .ok_or_else(|| KubeletError::Pod("Pod has no spec".to_string()))?;

        let container_spec = spec
            .containers
            .iter()
            .find(|c| c.name == container_name)
            .ok_or_else(|| KubeletError::Pod(format!("Container {} not found", container_name)))?;

        let container_id = format!(
            "k1s_{}_{}_{}",
            pod.metadata.effective_namespace(),
            pod.metadata.name,
            container_name
        );

        // Build labels
        let mut labels = HashMap::new();
        labels.insert(CONTAINER_LABEL_POD_UID.to_string(), pod.metadata.uid.clone());
        labels.insert(CONTAINER_LABEL_POD_NAME.to_string(), pod.metadata.name.clone());
        labels.insert(
            CONTAINER_LABEL_POD_NAMESPACE.to_string(),
            pod.metadata.effective_namespace().to_string(),
        );
        labels.insert(CONTAINER_LABEL_CONTAINER_NAME.to_string(), container_name.to_string());

        // Build OCI runtime spec with additional env vars
        let oci_spec = build_oci_spec(pod, container_spec, env_config)?;

        let mut client = ContainersClient::new(self.channel.clone());

        let container = Container {
            id: container_id.clone(),
            image: container_spec.image.clone(),
            labels,
            runtime: Some(proto::containerd::services::containers::v1::container::Runtime {
                name: "io.containerd.runc.v2".to_string(),
                options: None,
            }),
            spec: Some(prost_types::Any {
                type_url: "types.containerd.io/opencontainers/runtime-spec/1/Spec".to_string(),
                value: serde_json::to_vec(&oci_spec)
                    .map_err(|e| KubeletError::Runtime(format!("Failed to serialize spec: {}", e)))?,
            }),
            ..Default::default()
        };

        let request = self.with_namespace(tonic::Request::new(CreateContainerRequest {
            container: Some(container),
        }));

        client
            .create(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to create container: {}", e)))?;

        info!("Created container: {}", container_id);
        Ok(container_id)
    }

    async fn start_container(&self, container_id: &str) -> KubeletResult<()> {
        let mut client = TasksClient::new(self.channel.clone());

        // Create task first
        let request = self.with_namespace(tonic::Request::new(CreateTaskRequest {
            container_id: container_id.to_string(),
            ..Default::default()
        }));

        let response = client
            .create(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to create task: {}", e)))?;

        let pid = response.into_inner().pid;
        debug!("Created task with PID: {}", pid);

        // Start the task
        let request = self.with_namespace(tonic::Request::new(StartRequest {
            container_id: container_id.to_string(),
            ..Default::default()
        }));

        client
            .start(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to start task: {}", e)))?;

        info!("Started container: {}", container_id);
        Ok(())
    }

    async fn stop_container(&self, container_id: &str, timeout: u64) -> KubeletResult<()> {
        let mut client = TasksClient::new(self.channel.clone());

        // Send SIGTERM
        let request = self.with_namespace(tonic::Request::new(KillRequest {
            container_id: container_id.to_string(),
            signal: 15, // SIGTERM
            all: true,
            ..Default::default()
        }));

        if let Err(e) = client.kill(request).await {
            debug!("Kill request failed (may already be stopped): {}", e);
        }

        // Wait for container to stop
        tokio::time::sleep(Duration::from_secs(timeout.min(30))).await;

        // Force kill if still running
        let request = self.with_namespace(tonic::Request::new(KillRequest {
            container_id: container_id.to_string(),
            signal: 9, // SIGKILL
            all: true,
            ..Default::default()
        }));

        let _ = client.kill(request).await;

        info!("Stopped container: {}", container_id);
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> KubeletResult<()> {
        // Delete task first
        let mut tasks_client = TasksClient::new(self.channel.clone());
        let request = self.with_namespace(tonic::Request::new(DeleteTaskRequest {
            container_id: container_id.to_string(),
        }));
        let _ = tasks_client.delete(request).await;

        // Delete container
        let mut containers_client = ContainersClient::new(self.channel.clone());
        let request = self.with_namespace(tonic::Request::new(DeleteContainerRequest {
            id: container_id.to_string(),
        }));

        containers_client
            .delete(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to delete container: {}", e)))?;

        info!("Removed container: {}", container_id);
        Ok(())
    }

    async fn get_container(&self, container_id: &str) -> KubeletResult<Option<ContainerInfo>> {
        let mut containers_client = ContainersClient::new(self.channel.clone());

        let request = self.with_namespace(tonic::Request::new(GetContainerRequest {
            id: container_id.to_string(),
        }));

        match containers_client.get(request).await {
            Ok(response) => {
                let container = response.into_inner().container;
                if let Some(c) = container {
                    // Get task status
                    let state = self.get_task_state(container_id).await;

                    Ok(Some(ContainerInfo {
                        id: c.id,
                        name: c
                            .labels
                            .get(CONTAINER_LABEL_CONTAINER_NAME)
                            .cloned()
                            .unwrap_or_default(),
                        image: c.image,
                        state,
                        exit_code: None,
                    }))
                } else {
                    Ok(None)
                }
            }
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(e) => Err(KubeletError::Runtime(format!(
                "Failed to get container: {}",
                e
            ))),
        }
    }

    async fn list_containers(&self, pod_uid: &str) -> KubeletResult<Vec<ContainerInfo>> {
        let mut client = ContainersClient::new(self.channel.clone());

        let request = self.with_namespace(tonic::Request::new(ListContainersRequest {
            filters: vec![format!("labels.{}=={}", CONTAINER_LABEL_POD_UID, pod_uid)],
        }));

        let response = client
            .list(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to list containers: {}", e)))?;

        let mut result = Vec::new();
        for c in response.into_inner().containers {
            let state = self.get_task_state(&c.id).await;
            result.push(ContainerInfo {
                id: c.id.clone(),
                name: c
                    .labels
                    .get(CONTAINER_LABEL_CONTAINER_NAME)
                    .cloned()
                    .unwrap_or_default(),
                image: c.image,
                state,
                exit_code: None,
            });
        }

        Ok(result)
    }

    async fn logs(&self, container_id: &str, tail: Option<u32>) -> KubeletResult<String> {
        // containerd stores logs in the container's working directory
        // For a full implementation, we'd need to read from the log file
        // or use the streaming logs API

        // Simplified: use ctr to get logs
        let mut cmd = tokio::process::Command::new("ctr");
        cmd.args(["-n", &self.namespace, "tasks", "logs", container_id]);

        if let Some(n) = tail {
            cmd.args(["--tail", &n.to_string()]);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to get logs: {}", e)))?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
    ) -> KubeletResult<(i32, String, String)> {
        // Use ctr for exec as a simplified implementation
        let mut cmd = tokio::process::Command::new("ctr");
        cmd.args(["-n", &self.namespace, "tasks", "exec", "--exec-id", &uuid::Uuid::new_v4().to_string(), container_id]);
        cmd.args(command);

        let output = cmd
            .output()
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to exec: {}", e)))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((exit_code, stdout, stderr))
    }

    async fn list_images(&self) -> KubeletResult<Vec<ImageInfo>> {
        let mut client = ImagesClient::new(self.channel.clone());

        let request = self.with_namespace(tonic::Request::new(ListImagesRequest {
            filters: vec![],
        }));

        let response = client
            .list(request)
            .await
            .map_err(|e| KubeletError::Runtime(format!("Failed to list images: {}", e)))?;

        let result = response
            .into_inner()
            .images
            .into_iter()
            .map(|img| ImageInfo {
                id: img.name.clone(),
                tags: vec![img.name],
                size: 0, // Would need to query content store for size
            })
            .collect();

        Ok(result)
    }
}

impl ContainerdRuntime {
    async fn get_task_state(&self, container_id: &str) -> ContainerState {
        let mut client = TasksClient::new(self.channel.clone());

        let request = self.with_namespace(tonic::Request::new(GetRequest {
            container_id: container_id.to_string(),
            ..Default::default()
        }));

        match client.get(request).await {
            Ok(response) => {
                let process = response.into_inner().process;
                if let Some(p) = process {
                    match p.get_status() {
                        proto::containerd::services::tasks::v1::Status::Running => {
                            ContainerState::Running
                        }
                        proto::containerd::services::tasks::v1::Status::Created => {
                            ContainerState::Created
                        }
                        proto::containerd::services::tasks::v1::Status::Stopped => {
                            ContainerState::Stopped
                        }
                        proto::containerd::services::tasks::v1::Status::Paused => {
                            ContainerState::Paused
                        }
                        _ => ContainerState::Unknown,
                    }
                } else {
                    ContainerState::Unknown
                }
            }
            Err(_) => ContainerState::Unknown,
        }
    }
}

fn build_oci_spec(
    pod: &Pod,
    container: &k1s_types::Container,
    env_config: &ContainerEnvConfig,
) -> KubeletResult<serde_json::Value> {
    // Build a minimal OCI runtime spec
    let mut env: Vec<String> = container
        .env
        .iter()
        .filter_map(|e| e.value.as_ref().map(|v| format!("{}={}", e.name, v)))
        .collect();

    // Add resolved environment variables from ConfigMaps/Secrets
    env.extend(env_config.env_vars.clone());

    // Add standard K8s environment variables
    env.push("KUBERNETES_SERVICE_HOST=10.43.0.1".to_string());
    env.push("KUBERNETES_SERVICE_PORT=443".to_string());

    let mut args: Vec<String> = Vec::new();
    if !container.command.is_empty() {
        args.extend(container.command.clone());
    }
    if !container.args.is_empty() {
        args.extend(container.args.clone());
    }

    // Build volume mounts
    let mut mounts = vec![
        serde_json::json!({
            "destination": "/proc",
            "type": "proc",
            "source": "proc"
        }),
        serde_json::json!({
            "destination": "/dev",
            "type": "tmpfs",
            "source": "tmpfs",
            "options": ["nosuid", "strictatime", "mode=755", "size=65536k"]
        }),
        serde_json::json!({
            "destination": "/sys",
            "type": "sysfs",
            "source": "sysfs",
            "options": ["nosuid", "noexec", "nodev", "ro"]
        }),
    ];

    // Add ConfigMap/Secret volume mounts
    let data_dir = std::env::temp_dir().join("k1s-volumes").join(&pod.metadata.uid);
    for (mount_path, files) in &env_config.volume_data {
        let volume_name = mount_path.replace('/', "_").trim_matches('_').to_string();
        let host_path = data_dir.join(&volume_name);

        // Create directory and write files
        if let Err(e) = std::fs::create_dir_all(&host_path) {
            return Err(KubeletError::Runtime(format!(
                "Failed to create volume directory: {}",
                e
            )));
        }

        for (filename, content) in files {
            let file_path = host_path.join(filename);
            if let Err(e) = std::fs::write(&file_path, content) {
                return Err(KubeletError::Runtime(format!(
                    "Failed to write volume file {}: {}",
                    filename, e
                )));
            }
        }

        mounts.push(serde_json::json!({
            "destination": mount_path,
            "type": "bind",
            "source": host_path.to_string_lossy(),
            "options": ["rbind", "rprivate"]
        }));
    }

    // Build security context settings
    let pod_security = pod.spec.as_ref().and_then(|s| s.security_context.as_ref());
    let container_security = container.security_context.as_ref();

    // Determine user/group to run as (container-level overrides pod-level)
    let uid = container_security
        .and_then(|s| s.run_as_user)
        .or_else(|| pod_security.and_then(|s| s.run_as_user))
        .unwrap_or(0) as u32;
    let gid = container_security
        .and_then(|s| s.run_as_group)
        .or_else(|| pod_security.and_then(|s| s.run_as_group))
        .unwrap_or(0) as u32;

    // Read-only root filesystem
    let readonly_rootfs = container_security
        .and_then(|s| s.read_only_root_filesystem)
        .unwrap_or(false);

    // Build capabilities
    let mut capabilities = serde_json::json!({
        "bounding": ["CAP_AUDIT_WRITE", "CAP_KILL", "CAP_NET_BIND_SERVICE"],
        "effective": ["CAP_AUDIT_WRITE", "CAP_KILL", "CAP_NET_BIND_SERVICE"],
        "inheritable": [],
        "permitted": ["CAP_AUDIT_WRITE", "CAP_KILL", "CAP_NET_BIND_SERVICE"],
        "ambient": []
    });

    if let Some(caps) = container_security.and_then(|s| s.capabilities.as_ref()) {
        // Add capabilities
        for cap in &caps.add {
            let cap_name = if cap.starts_with("CAP_") { cap.clone() } else { format!("CAP_{}", cap) };
            for field in ["bounding", "effective", "permitted"] {
                if let Some(arr) = capabilities[field].as_array_mut() {
                    if !arr.iter().any(|v| v.as_str() == Some(&cap_name)) {
                        arr.push(serde_json::Value::String(cap_name.clone()));
                    }
                }
            }
        }
        // Drop capabilities
        for cap in &caps.drop {
            let cap_name = if cap.starts_with("CAP_") { cap.clone() } else { format!("CAP_{}", cap) };
            for field in ["bounding", "effective", "permitted", "ambient", "inheritable"] {
                if let Some(arr) = capabilities[field].as_array_mut() {
                    arr.retain(|v| v.as_str() != Some(&cap_name));
                }
            }
        }
    }

    let spec = serde_json::json!({
        "ociVersion": "1.0.2",
        "process": {
            "terminal": container.tty,
            "user": {
                "uid": uid,
                "gid": gid
            },
            "args": if args.is_empty() { vec!["/bin/sh".to_string()] } else { args },
            "env": env,
            "cwd": container.working_dir.as_deref().unwrap_or("/"),
            "capabilities": capabilities,
        },
        "root": {
            "path": "rootfs",
            "readonly": readonly_rootfs
        },
        "hostname": pod.spec.as_ref().and_then(|s| s.hostname.clone()).unwrap_or_else(|| pod.metadata.name.clone()),
        "mounts": mounts,
        "linux": {
            "namespaces": [
                { "type": "pid" },
                { "type": "ipc" },
                { "type": "uts" },
                { "type": "mount" },
                { "type": "network" }
            ]
        }
    });

    Ok(spec)
}
