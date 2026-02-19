//! Native Docker Compose executor
//!
//! Executes compose services directly as Docker containers without going
//! through the Kubernetes API. This enables `k1s apply -f docker-compose.yml`
//! to work in standalone mode.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::models::{HostConfig, Mount, MountTypeEnum, PortBinding, RestartPolicy, RestartPolicyNameEnum};
use bollard::network::CreateNetworkOptions;
use bollard::Docker;
use futures::StreamExt;
use tracing::{debug, info};

use super::parser::{ComposeFile, ComposeDependsOn, ComposeService};

/// Labels applied to compose-managed containers for identification
mod labels {
    pub const PROJECT: &str = "com.docker.compose.project";
    pub const SERVICE: &str = "com.docker.compose.service";
    pub const CONTAINER_NUMBER: &str = "com.docker.compose.container-number";
    pub const VERSION: &str = "com.docker.compose.version";
}

/// Container info for display purposes
#[derive(Debug, Clone)]
pub struct ComposeContainerInfo {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub ports: String,
}

/// Orchestrates Docker Compose services using native Docker operations
pub struct ComposeExecutor {
    docker: Docker,
    project_name: String,
    working_dir: Option<String>,
}

impl ComposeExecutor {
    /// Create a new executor connected to the Docker daemon
    pub async fn new(project_name: &str) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults()
            .context("Failed to connect to Docker daemon")?;

        docker.ping().await.context("Docker daemon not responding")?;

        Ok(Self {
            docker,
            project_name: project_name.to_string(),
            working_dir: None,
        })
    }

    /// Set the working directory for resolving relative paths
    pub fn with_working_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.working_dir = Some(dir.as_ref().to_string_lossy().to_string());
        self
    }

    /// Bring up all services defined in the compose file
    pub async fn up(&self, compose: &ComposeFile, detach: bool) -> Result<()> {
        self.ensure_networks(compose).await?;
        self.ensure_volumes(compose).await?;

        let start_order = self.resolve_start_order(compose)?;

        for service_name in &start_order {
            let service = compose.services.get(service_name).unwrap();
            self.up_service(service_name, service).await?;
        }

        if detach {
            info!("Services started in detached mode");
        }

        Ok(())
    }

    /// Bring down all services for this project
    pub async fn down(&self, compose: &ComposeFile, remove_volumes: bool) -> Result<()> {
        let containers = self.list_project_containers().await?;

        for container in containers {
            info!("Stopping container {}", container.name);
            self.docker
                .stop_container(&container.id, Some(StopContainerOptions { t: 10 }))
                .await
                .ok();

            info!("Removing container {}", container.name);
            self.docker
                .remove_container(
                    &container.id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: remove_volumes,
                        ..Default::default()
                    }),
                )
                .await
                .ok();
        }

        self.cleanup_networks(compose).await?;

        if remove_volumes {
            self.cleanup_volumes(compose).await?;
        }

        Ok(())
    }

    /// List containers for this project
    pub async fn ps(&self) -> Result<Vec<ComposeContainerInfo>> {
        self.list_project_containers().await
    }

    async fn ensure_networks(&self, compose: &ComposeFile) -> Result<()> {
        let default_network = format!("{}_default", self.project_name);

        if !self.network_exists(&default_network).await? {
            info!("Creating network {}", default_network);
            self.docker
                .create_network(CreateNetworkOptions {
                    name: default_network.clone(),
                    driver: "bridge".to_string(),
                    ..Default::default()
                })
                .await
                .context("Failed to create default network")?;
        }

        for (name, network) in &compose.networks {
            if network.external.unwrap_or(false) {
                continue;
            }

            let network_name = network
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_{}", self.project_name, name));

            if !self.network_exists(&network_name).await? {
                info!("Creating network {}", network_name);
                self.docker
                    .create_network(CreateNetworkOptions {
                        name: network_name,
                        driver: network.driver.clone().unwrap_or_else(|| "bridge".to_string()),
                        ..Default::default()
                    })
                    .await
                    .context("Failed to create network")?;
            }
        }

        Ok(())
    }

    async fn network_exists(&self, name: &str) -> Result<bool> {
        match self.docker.inspect_network::<String>(name, None).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(false),
            Err(e) => Err(e).context("Failed to check network"),
        }
    }

    async fn cleanup_networks(&self, compose: &ComposeFile) -> Result<()> {
        let default_network = format!("{}_default", self.project_name);

        if self.network_exists(&default_network).await? {
            info!("Removing network {}", default_network);
            self.docker.remove_network(&default_network).await.ok();
        }

        for (name, network) in &compose.networks {
            if network.external.unwrap_or(false) {
                continue;
            }

            let network_name = network
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_{}", self.project_name, name));

            if self.network_exists(&network_name).await? {
                info!("Removing network {}", network_name);
                self.docker.remove_network(&network_name).await.ok();
            }
        }

        Ok(())
    }

    async fn ensure_volumes(&self, compose: &ComposeFile) -> Result<()> {
        for (name, volume) in &compose.volumes {
            if volume.external.unwrap_or(false) {
                continue;
            }

            let volume_name = volume
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_{}", self.project_name, name));

            match self.docker.inspect_volume(&volume_name).await {
                Ok(_) => debug!("Volume {} exists", volume_name),
                Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                    info!("Creating volume {}", volume_name);
                    self.docker
                        .create_volume(bollard::volume::CreateVolumeOptions {
                            name: volume_name.clone(),
                            driver: volume.driver.clone().unwrap_or_else(|| "local".to_string()),
                            ..Default::default()
                        })
                        .await
                        .context("Failed to create volume")?;
                }
                Err(e) => return Err(e).context("Failed to check volume"),
            }
        }

        Ok(())
    }

    async fn cleanup_volumes(&self, compose: &ComposeFile) -> Result<()> {
        for (name, volume) in &compose.volumes {
            if volume.external.unwrap_or(false) {
                continue;
            }

            let volume_name = volume
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_{}", self.project_name, name));

            info!("Removing volume {}", volume_name);
            self.docker.remove_volume(&volume_name, None).await.ok();
        }

        Ok(())
    }

    async fn up_service(&self, service_name: &str, service: &ComposeService) -> Result<()> {
        let image = service.image.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Service {} has no image (build not supported)", service_name)
        })?;

        self.ensure_image(image).await?;

        let replicas = service
            .deploy
            .as_ref()
            .and_then(|d| d.replicas)
            .unwrap_or(1);

        for i in 1..=replicas {
            let container_name = if replicas == 1 {
                format!("{}-{}-1", self.project_name, service_name)
            } else {
                format!("{}-{}-{}", self.project_name, service_name, i)
            };

            if self.container_exists(&container_name).await? {
                let info = self.docker.inspect_container(&container_name, None).await?;
                if info.state.and_then(|s| s.running).unwrap_or(false) {
                    info!("Container {} is already running", container_name);
                    continue;
                }
                info!("Starting existing container {}", container_name);
                self.docker
                    .start_container(&container_name, None::<StartContainerOptions<String>>)
                    .await?;
                continue;
            }

            info!("Creating container {}", container_name);
            let config = self.build_container_config(service_name, service, i)?;

            self.docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: &container_name,
                        platform: None,
                    }),
                    config,
                )
                .await
                .context("Failed to create container")?;

            info!("Starting container {}", container_name);
            self.docker
                .start_container(&container_name, None::<StartContainerOptions<String>>)
                .await
                .context("Failed to start container")?;
        }

        Ok(())
    }

    async fn ensure_image(&self, image: &str) -> Result<()> {
        match self.docker.inspect_image(image).await {
            Ok(_) => {
                debug!("Image {} exists locally", image);
                Ok(())
            }
            Err(_) => {
                info!("Pulling image {}", image);
                let options = bollard::image::CreateImageOptions {
                    from_image: image,
                    ..Default::default()
                };

                let mut stream = self.docker.create_image(Some(options), None, None);
                while let Some(result) = stream.next().await {
                    result.context("Failed to pull image")?;
                }

                Ok(())
            }
        }
    }

    async fn container_exists(&self, name: &str) -> Result<bool> {
        match self.docker.inspect_container(name, None).await {
            Ok(_) => Ok(true),
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => Ok(false),
            Err(e) => Err(e).context("Failed to check container"),
        }
    }

    fn build_container_config(
        &self,
        service_name: &str,
        service: &ComposeService,
        replica: u32,
    ) -> Result<Config<String>> {
        let image = service.image.clone().unwrap_or_else(|| format!("{}:latest", service_name));

        let mut labels = HashMap::new();
        labels.insert(labels::PROJECT.to_string(), self.project_name.clone());
        labels.insert(labels::SERVICE.to_string(), service_name.to_string());
        labels.insert(labels::CONTAINER_NUMBER.to_string(), replica.to_string());
        labels.insert(labels::VERSION.to_string(), "k1s".to_string());

        if let Some(compose_labels) = &service.labels {
            for (k, v) in compose_labels.to_map() {
                labels.insert(k, v);
            }
        }

        let env: Option<Vec<String>> = service.environment.as_ref().map(|e| {
            e.to_map().into_iter().map(|(k, v)| format!("{}={}", k, v)).collect()
        });

        let cmd = service.command.as_ref().map(|c| c.to_vec());
        let entrypoint = service.entrypoint.as_ref().map(|e| e.to_vec());

        let exposed_ports = self.build_exposed_ports(service);
        let host_config = self.build_host_config(service)?;

        Ok(Config {
            image: Some(image),
            labels: Some(labels),
            env,
            cmd,
            entrypoint,
            exposed_ports,
            host_config: Some(host_config),
            working_dir: service.working_dir.clone(),
            user: service.user.clone(),
            hostname: service.hostname.clone(),
            ..Default::default()
        })
    }

    fn build_exposed_ports(&self, service: &ComposeService) -> Option<HashMap<String, HashMap<(), ()>>> {
        if service.ports.is_empty() && service.expose.is_empty() {
            return None;
        }

        let mut exposed = HashMap::new();

        for port in &service.ports {
            let (_, container_port, protocol) = port.parse();
            let key = format!("{}/{}", container_port, protocol);
            exposed.insert(key, HashMap::new());
        }

        for expose in &service.expose {
            let key = format!("{}/tcp", expose);
            exposed.insert(key, HashMap::new());
        }

        Some(exposed)
    }

    fn build_host_config(&self, service: &ComposeService) -> Result<HostConfig> {
        let port_bindings = self.build_port_bindings(service);
        let mounts = self.build_mounts(service)?;
        let network_mode = Some(format!("{}_default", self.project_name));

        let restart_policy = service.restart.as_ref().map(|r| {
            let name = match r.as_str() {
                "no" => RestartPolicyNameEnum::NO,
                "always" => RestartPolicyNameEnum::ALWAYS,
                "on-failure" => RestartPolicyNameEnum::ON_FAILURE,
                "unless-stopped" => RestartPolicyNameEnum::UNLESS_STOPPED,
                _ => RestartPolicyNameEnum::NO,
            };
            RestartPolicy {
                name: Some(name),
                maximum_retry_count: None,
            }
        });

        let privileged = service.privileged;

        let cap_add = if service.cap_add.is_empty() {
            None
        } else {
            Some(service.cap_add.clone())
        };

        let cap_drop = if service.cap_drop.is_empty() {
            None
        } else {
            Some(service.cap_drop.clone())
        };

        let extra_hosts = if service.extra_hosts.is_empty() {
            None
        } else {
            Some(service.extra_hosts.clone())
        };

        Ok(HostConfig {
            port_bindings,
            mounts,
            network_mode,
            restart_policy,
            privileged,
            cap_add,
            cap_drop,
            extra_hosts,
            ..Default::default()
        })
    }

    fn build_port_bindings(&self, service: &ComposeService) -> Option<HashMap<String, Option<Vec<PortBinding>>>> {
        if service.ports.is_empty() {
            return None;
        }

        let mut bindings = HashMap::new();

        for port in &service.ports {
            let (host_port, container_port, protocol) = port.parse();
            let key = format!("{}/{}", container_port, protocol);

            let binding = PortBinding {
                host_ip: Some("0.0.0.0".to_string()),
                host_port: host_port.map(|p| p.to_string()),
            };

            bindings.insert(key, Some(vec![binding]));
        }

        Some(bindings)
    }

    fn build_mounts(&self, service: &ComposeService) -> Result<Option<Vec<Mount>>> {
        if service.volumes.is_empty() {
            return Ok(None);
        }

        let mounts: Vec<Mount> = service
            .volumes
            .iter()
            .map(|v| {
                let (source, target, read_only) = v.parse();

                match source {
                    Some(src) if src.starts_with('/') || src.starts_with('.') => {
                        let abs_source = if src.starts_with('.') {
                            self.working_dir
                                .as_ref()
                                .map(|wd| format!("{}/{}", wd, &src[1..]))
                                .unwrap_or_else(|| src.clone())
                        } else {
                            src
                        };

                        Mount {
                            target: Some(target),
                            source: Some(abs_source),
                            typ: Some(MountTypeEnum::BIND),
                            read_only: Some(read_only),
                            ..Default::default()
                        }
                    }
                    Some(src) => Mount {
                        target: Some(target),
                        source: Some(format!("{}_{}", self.project_name, src)),
                        typ: Some(MountTypeEnum::VOLUME),
                        read_only: Some(read_only),
                        ..Default::default()
                    },
                    None => Mount {
                        target: Some(target),
                        typ: Some(MountTypeEnum::VOLUME),
                        read_only: Some(read_only),
                        ..Default::default()
                    },
                }
            })
            .collect();

        Ok(Some(mounts))
    }

    fn resolve_start_order(&self, compose: &ComposeFile) -> Result<Vec<String>> {
        topological_sort(compose)
    }

    async fn list_project_containers(&self) -> Result<Vec<ComposeContainerInfo>> {
        let label_filter = format!("{}={}", labels::PROJECT, self.project_name);
        let mut filters = HashMap::new();
        filters.insert("label", vec![label_filter.as_str()]);

        let containers = self
            .docker
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .context("Failed to list containers")?;

        let result: Vec<_> = containers
            .into_iter()
            .map(|c| {
                let _labels = c.labels.unwrap_or_default();

                let ports = c
                    .ports
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|p| {
                        p.public_port.map(|pub_port| {
                            format!(
                                "{}:{}->{}",
                                p.ip.unwrap_or_else(|| "0.0.0.0".to_string()),
                                pub_port,
                                p.private_port
                            )
                        })
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                ComposeContainerInfo {
                    id: c.id.unwrap_or_default().chars().take(12).collect(),
                    name: c
                        .names
                        .and_then(|n| n.first().cloned())
                        .unwrap_or_default()
                        .trim_start_matches('/')
                        .to_string(),
                    image: c.image.unwrap_or_default(),
                    state: c.state.unwrap_or_default(),
                    ports,
                }
            })
            .collect();

        Ok(result)
    }
}

/// Compute start order for services respecting depends_on constraints.
/// Returns services in dependency order (dependencies before dependents).
fn topological_sort(compose: &ComposeFile) -> Result<Vec<String>> {
    let mut order = Vec::new();
    let mut visited = BTreeMap::new();

    for service_name in compose.services.keys() {
        topo_visit(service_name, compose, &mut visited, &mut order)?;
    }

    Ok(order)
}

fn topo_visit(
    name: &str,
    compose: &ComposeFile,
    visited: &mut BTreeMap<String, bool>,
    order: &mut Vec<String>,
) -> Result<()> {
    if let Some(&in_progress) = visited.get(name) {
        if in_progress {
            anyhow::bail!("Circular dependency detected involving service: {}", name);
        }
        return Ok(());
    }

    visited.insert(name.to_string(), true);

    if let Some(service) = compose.services.get(name) {
        let deps = match &service.depends_on {
            Some(ComposeDependsOn::List(list)) => list.clone(),
            Some(ComposeDependsOn::Map(map)) => map.keys().cloned().collect(),
            None => vec![],
        };

        for dep in deps {
            topo_visit(&dep, compose, visited, order)?;
        }
    }

    visited.insert(name.to_string(), false);

    if !order.contains(&name.to_string()) {
        order.push(name.to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topo_sort_simple() {
        let yaml = r#"
version: "3"
services:
  web:
    image: nginx
    depends_on:
      - api
      - db
  api:
    image: node
    depends_on:
      - db
  db:
    image: postgres
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let order = topological_sort(&compose).unwrap();

        let db_idx = order.iter().position(|s| s == "db").unwrap();
        let api_idx = order.iter().position(|s| s == "api").unwrap();
        let web_idx = order.iter().position(|s| s == "web").unwrap();

        assert!(db_idx < api_idx);
        assert!(db_idx < web_idx);
        assert!(api_idx < web_idx);
    }

    #[test]
    fn test_topo_sort_circular_dep() {
        let yaml = r#"
version: "3"
services:
  a:
    image: alpine
    depends_on:
      - b
  b:
    image: alpine
    depends_on:
      - a
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let result = topological_sort(&compose);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Circular dependency"));
    }

    #[test]
    fn test_topo_sort_no_deps() {
        let yaml = r#"
version: "3"
services:
  a:
    image: alpine
  b:
    image: alpine
  c:
    image: alpine
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let order = topological_sort(&compose).unwrap();
        assert_eq!(order.len(), 3);
    }
}
