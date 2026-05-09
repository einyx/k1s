//! Docker runtime implementation using bollard

use async_trait::async_trait;
use bollard::container::{Config, CreateContainerOptions, LogsOptions, StartContainerOptions, StopContainerOptions, RemoveContainerOptions, ListContainersOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::{CreateImageOptions, ListImagesOptions};
use bollard::models::{HostConfig, Mount, MountTypeEnum};
use bollard::Docker;
use futures::StreamExt;
use k1s_types::Pod;
use std::collections::HashMap;
use tracing::{debug, info};

use super::{ContainerInfo, ContainerRuntime, ContainerState, ContainerEnvConfig, ImageInfo, RuntimeConfig};
use crate::{KubeletError, KubeletResult};

pub struct DockerRuntime {
    client: Docker,
}

impl DockerRuntime {
    pub async fn new(config: &RuntimeConfig) -> KubeletResult<Self> {
        // Try to connect using the configured socket path, or auto-detect
        let client = Self::connect(&config.socket_path).await?;

        // Verify connection
        client.ping().await.map_err(|e| KubeletError::Runtime(e.to_string()))?;

        info!("Connected to Docker");
        Ok(Self { client })
    }

    async fn connect(socket_path: &str) -> KubeletResult<Docker> {
        // If explicit path is given and exists, use it
        if socket_path.starts_with("unix://") || socket_path.starts_with('/') {
            let path = socket_path.strip_prefix("unix://").unwrap_or(socket_path);
            if std::path::Path::new(path).exists() {
                return Docker::connect_with_socket(socket_path, 120, bollard::API_DEFAULT_VERSION)
                    .map_err(|e| KubeletError::Runtime(e.to_string()));
            }
        }

        // Try common Docker socket locations in order
        let socket_paths = [
            // Linux default
            "/var/run/docker.sock",
            // macOS Docker Desktop
            &format!("{}/.docker/run/docker.sock", std::env::var("HOME").unwrap_or_default()),
            // Alternative macOS location
            "/Users/Shared/docker/docker.sock",
            // Colima on macOS
            &format!("{}/.colima/default/docker.sock", std::env::var("HOME").unwrap_or_default()),
            // Podman socket
            &format!("{}/podman/podman.sock", std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/run/user/1000".to_string())),
        ];

        for path in &socket_paths {
            if std::path::Path::new(path).exists() {
                debug!("Trying Docker socket at: {}", path);
                if let Ok(client) = Docker::connect_with_socket(path, 120, bollard::API_DEFAULT_VERSION) {
                    if client.ping().await.is_ok() {
                        info!("Using Docker socket: {}", path);
                        return Ok(client);
                    }
                }
            }
        }

        // Fall back to bollard's default detection (uses DOCKER_HOST env var)
        Docker::connect_with_local_defaults()
            .map_err(|e| KubeletError::Runtime(format!("Failed to connect to Docker. Tried common socket paths. Error: {e}")))
    }
}

#[async_trait]
impl ContainerRuntime for DockerRuntime {
    fn name(&self) -> &str {
        "docker"
    }

    async fn pull_image(&self, image: &str) -> KubeletResult<()> {
        let options = Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        });

        let mut stream = self.client.create_image(options, None, None);
        while let Some(result) = stream.next().await {
            result.map_err(|e| KubeletError::Runtime(e.to_string()))?;
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
        let spec = pod.spec.as_ref().ok_or_else(|| {
            KubeletError::Pod("Pod has no spec".to_string())
        })?;

        let container_spec = spec
            .containers
            .iter()
            .find(|c| c.name == container_name)
            .ok_or_else(|| KubeletError::Pod(format!("Container {container_name} not found")))?;

        // Build environment variables from container spec
        let mut env: Vec<String> = container_spec
            .env
            .iter()
            .filter_map(|e| e.value.as_ref().map(|v| format!("{}={}", e.name, v)))
            .collect();

        // Add resolved environment variables from ConfigMaps/Secrets
        env.extend(env_config.env_vars.clone());

        // Build labels
        let mut labels = HashMap::new();
        labels.insert("k1s.pod.name".to_string(), pod.metadata.name.clone());
        labels.insert("k1s.pod.namespace".to_string(), pod.metadata.effective_namespace().to_string());
        labels.insert("k1s.pod.uid".to_string(), pod.metadata.uid.clone());
        labels.insert("k1s.container.name".to_string(), container_name.to_string());

        // Create volume mounts for ConfigMap/Secret data
        let mut mounts = Vec::new();
        let data_dir = std::env::temp_dir().join("k1s-volumes").join(&pod.metadata.uid);

        for (mount_path, files) in &env_config.volume_data {
            // Create host directory for this volume
            let volume_name = mount_path.replace('/', "_").trim_matches('_').to_string();
            let host_path = data_dir.join(&volume_name);

            if let Err(e) = std::fs::create_dir_all(&host_path) {
                return Err(KubeletError::Runtime(format!(
                    "Failed to create volume directory: {e}"
                )));
            }

            // Write files
            for (filename, content) in files {
                let file_path = host_path.join(filename);
                if let Err(e) = std::fs::write(&file_path, content) {
                    return Err(KubeletError::Runtime(format!(
                        "Failed to write volume file {filename}: {e}"
                    )));
                }
                debug!("Wrote volume file: {:?}", file_path);
            }

            mounts.push(Mount {
                target: Some(mount_path.clone()),
                source: Some(host_path.to_string_lossy().to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(container_spec.volume_mounts.iter()
                    .find(|vm| vm.mount_path == *mount_path)
                    .is_some_and(|vm| vm.read_only)),
                ..Default::default()
            });
        }

        // Add hostPath volumes
        for volume_mount in &container_spec.volume_mounts {
            if let Some(volume) = spec.volumes.iter().find(|v| v.name == volume_mount.name) {
                if let Some(host_path) = &volume.source.host_path {
                    mounts.push(Mount {
                        target: Some(volume_mount.mount_path.clone()),
                        source: Some(host_path.path.clone()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(volume_mount.read_only),
                        ..Default::default()
                    });
                } else if let Some(_empty_dir) = &volume.source.empty_dir {
                    // Create emptyDir volume
                    let host_path = data_dir.join(format!("emptydir-{}", volume_mount.name));
                    if let Err(e) = std::fs::create_dir_all(&host_path) {
                        return Err(KubeletError::Runtime(format!(
                            "Failed to create emptyDir: {e}"
                        )));
                    }
                    mounts.push(Mount {
                        target: Some(volume_mount.mount_path.clone()),
                        source: Some(host_path.to_string_lossy().to_string()),
                        typ: Some(MountTypeEnum::BIND),
                        read_only: Some(volume_mount.read_only),
                        ..Default::default()
                    });
                }
            }
        }

        // Build security context settings
        // Merge pod-level and container-level security context (container takes precedence)
        let pod_security = spec.security_context.as_ref();
        let container_security = container_spec.security_context.as_ref();

        // Determine user to run as (container-level overrides pod-level)
        let run_as_user = container_security
            .and_then(|s| s.run_as_user)
            .or_else(|| pod_security.and_then(|s| s.run_as_user));
        let run_as_group = container_security
            .and_then(|s| s.run_as_group)
            .or_else(|| pod_security.and_then(|s| s.run_as_group));

        // Build user string (uid:gid format)
        let user = match (run_as_user, run_as_group) {
            (Some(uid), Some(gid)) => Some(format!("{uid}:{gid}")),
            (Some(uid), None) => Some(uid.to_string()),
            _ => None,
        };

        // Security options
        let privileged = container_security.and_then(|s| s.privileged);
        let read_only_rootfs = container_security.and_then(|s| s.read_only_root_filesystem);

        // Capabilities
        let (cap_add, cap_drop) = if let Some(caps) = container_security.and_then(|s| s.capabilities.as_ref()) {
            (
                if caps.add.is_empty() { None } else { Some(caps.add.clone()) },
                if caps.drop.is_empty() { None } else { Some(caps.drop.clone()) },
            )
        } else {
            (None, None)
        };

        let host_config = Some(HostConfig {
            mounts: if mounts.is_empty() { None } else { Some(mounts) },
            privileged,
            readonly_rootfs: read_only_rootfs,
            cap_add,
            cap_drop,
            ..Default::default()
        });

        let config = Config {
            image: Some(container_spec.image.clone()),
            env: Some(env),
            labels: Some(labels),
            user,
            cmd: if container_spec.command.is_empty() {
                None
            } else {
                Some(container_spec.command.clone())
            },
            host_config,
            ..Default::default()
        };

        let container_full_name = format!(
            "k1s_{}_{}_{}",
            pod.metadata.effective_namespace(),
            pod.metadata.name,
            container_name
        );

        let options = Some(CreateContainerOptions {
            name: &container_full_name,
            platform: None,
        });

        let response = self
            .client
            .create_container(options, config)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        info!("Created container: {}", response.id);
        Ok(response.id)
    }

    async fn start_container(&self, container_id: &str) -> KubeletResult<()> {
        self.client
            .start_container(container_id, None::<StartContainerOptions<String>>)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        info!("Started container: {}", container_id);
        Ok(())
    }

    async fn stop_container(&self, container_id: &str, timeout: u64) -> KubeletResult<()> {
        let options = Some(StopContainerOptions { t: timeout as i64 });

        self.client
            .stop_container(container_id, options)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        info!("Stopped container: {}", container_id);
        Ok(())
    }

    async fn remove_container(&self, container_id: &str) -> KubeletResult<()> {
        let options = Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        });

        self.client
            .remove_container(container_id, options)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        info!("Removed container: {}", container_id);
        Ok(())
    }

    async fn get_container(&self, container_id: &str) -> KubeletResult<Option<ContainerInfo>> {
        match self.client.inspect_container(container_id, None).await {
            Ok(info) => {
                let state = info
                    .state
                    .as_ref()
                    .and_then(|s| s.running)
                    .map(|running| {
                        if running {
                            ContainerState::Running
                        } else {
                            ContainerState::Stopped
                        }
                    })
                    .unwrap_or(ContainerState::Unknown);

                Ok(Some(ContainerInfo {
                    id: info.id.unwrap_or_default(),
                    name: info.name.unwrap_or_default(),
                    image: info.config.and_then(|c| c.image).unwrap_or_default(),
                    state,
                    exit_code: info.state.and_then(|s| s.exit_code).map(|c| c as i32),
                }))
            }
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                Ok(None)
            }
            Err(e) => Err(KubeletError::Runtime(e.to_string())),
        }
    }

    async fn list_containers(&self, pod_uid: &str) -> KubeletResult<Vec<ContainerInfo>> {
        let label_filter = format!("k1s.pod.uid={pod_uid}");
        let mut filters = HashMap::new();
        filters.insert("label", vec![label_filter.as_str()]);

        let options = Some(ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        });

        let containers = self
            .client
            .list_containers(options)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        let result = containers
            .into_iter()
            .map(|c| {
                let state = match c.state.as_deref() {
                    Some("created") => ContainerState::Created,
                    Some("running") => ContainerState::Running,
                    Some("paused") => ContainerState::Paused,
                    Some("exited") => ContainerState::Stopped,
                    _ => ContainerState::Unknown,
                };

                ContainerInfo {
                    id: c.id.unwrap_or_default(),
                    name: c.names.and_then(|n| n.first().cloned()).unwrap_or_default(),
                    image: c.image.unwrap_or_default(),
                    state,
                    exit_code: None,
                }
            })
            .collect();

        Ok(result)
    }

    async fn logs(&self, container_id: &str, tail: Option<u32>) -> KubeletResult<String> {
        let options = Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail.map(|t| t.to_string()).unwrap_or_else(|| "all".to_string()),
            ..Default::default()
        });

        let mut stream = self.client.logs(container_id, options);
        let mut output = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(log) => output.push_str(&log.to_string()),
                Err(e) => return Err(KubeletError::Runtime(e.to_string())),
            }
        }

        Ok(output)
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
    ) -> KubeletResult<(i32, String, String)> {
        let exec = self
            .client
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(command.to_vec()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        let output = self
            .client
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = output {
            while let Some(result) = output.next().await {
                match result {
                    Ok(log) => match log {
                        bollard::container::LogOutput::StdOut { message } => {
                            stdout.push_str(&String::from_utf8_lossy(&message));
                        }
                        bollard::container::LogOutput::StdErr { message } => {
                            stderr.push_str(&String::from_utf8_lossy(&message));
                        }
                        _ => {}
                    },
                    Err(e) => return Err(KubeletError::Runtime(e.to_string())),
                }
            }
        }

        // Get exit code
        let inspect = self
            .client
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        let exit_code = inspect.exit_code.unwrap_or(0) as i32;

        Ok((exit_code, stdout, stderr))
    }

    async fn list_images(&self) -> KubeletResult<Vec<ImageInfo>> {
        let options = Some(ListImagesOptions::<String> {
            all: false,
            ..Default::default()
        });

        let images = self
            .client
            .list_images(options)
            .await
            .map_err(|e| KubeletError::Runtime(e.to_string()))?;

        let result = images
            .into_iter()
            .map(|i| ImageInfo {
                id: i.id,
                tags: i.repo_tags,
                size: i.size as u64,
            })
            .collect();

        Ok(result)
    }
}
