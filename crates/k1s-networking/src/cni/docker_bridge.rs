//! Docker-based CNI bridge plugin
//!
//! Uses Docker's network API to create a bridge network for pod connectivity.
//! Works on macOS and any platform with Docker.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::RwLock;

use bollard::network::{ConnectNetworkOptions, CreateNetworkOptions, DisconnectNetworkOptions};
use bollard::models::{EndpointSettings, EndpointIpamConfig, Ipam, IpamConfig};
use bollard::Docker;
use ipnetwork::Ipv4Network;
use tracing::{debug, info, warn};

use super::{CniDns, CniIp, CniResult, CniRoute};
use crate::{NetworkError, NetworkResult};

/// Docker-based CNI configuration
#[derive(Debug, Clone)]
pub struct DockerBridgeConfig {
    /// Name of the Docker network
    pub network_name: String,
    /// Subnet for pod IPs (e.g., "10.42.0.0/24")
    pub subnet: Ipv4Network,
    /// Gateway IP (usually .1 of subnet)
    pub gateway: Ipv4Addr,
    /// Docker socket path (optional)
    pub socket_path: Option<String>,
}

impl Default for DockerBridgeConfig {
    fn default() -> Self {
        let subnet = "10.42.0.0/24".parse().unwrap();
        Self {
            network_name: "k1s".to_string(),
            subnet,
            gateway: "10.42.0.1".parse().unwrap(),
            socket_path: None,
        }
    }
}

impl From<super::BridgeConfig> for DockerBridgeConfig {
    fn from(config: super::BridgeConfig) -> Self {
        Self {
            network_name: "k1s".to_string(),
            subnet: config.subnet,
            gateway: config.gateway,
            socket_path: None,
        }
    }
}

/// Docker-based CNI bridge plugin
pub struct DockerBridgeCni {
    config: DockerBridgeConfig,
    docker: Docker,
    /// Map of container_id -> allocated IP
    allocations: RwLock<HashMap<String, Ipv4Addr>>,
    /// Next IP to try allocating
    next_ip: RwLock<u32>,
}

impl DockerBridgeCni {
    /// Create a new Docker bridge CNI
    pub fn new(config: DockerBridgeConfig) -> NetworkResult<Self> {
        // Try multiple socket paths for Docker
        let docker = Self::connect_to_docker(&config.socket_path)?;

        let start_ip = u32::from(config.subnet.ip()) + 2;

        Ok(Self {
            config,
            docker,
            allocations: RwLock::new(HashMap::new()),
            next_ip: RwLock::new(start_ip),
        })
    }

    /// Connect to Docker, trying multiple socket paths
    fn connect_to_docker(socket_path: &Option<String>) -> NetworkResult<Docker> {
        // If socket path is provided, use it
        if let Some(path) = socket_path {
            return Docker::connect_with_socket(path, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| NetworkError::Cni(format!("Failed to connect to Docker at {}: {}", path, e)));
        }

        // Try common Docker socket locations
        let socket_paths = [
            // Docker Desktop on macOS (user-specific)
            format!("{}/.docker/run/docker.sock", std::env::var("HOME").unwrap_or_default()),
            // Standard Linux location
            "/var/run/docker.sock".to_string(),
            // Alternative macOS location
            "/var/run/docker.sock".to_string(),
            // Colima on macOS
            format!("{}/.colima/default/docker.sock", std::env::var("HOME").unwrap_or_default()),
        ];

        for path in &socket_paths {
            if std::path::Path::new(path).exists() {
                match Docker::connect_with_socket(path, 120, bollard::API_DEFAULT_VERSION) {
                    Ok(docker) => {
                        info!("Connected to Docker via {}", path);
                        return Ok(docker);
                    }
                    Err(e) => {
                        debug!("Failed to connect to Docker at {}: {}", path, e);
                    }
                }
            }
        }

        // Fall back to local defaults
        Docker::connect_with_local_defaults()
            .map_err(|e| NetworkError::Cni(format!("Failed to connect to Docker: {}", e)))
    }

    /// Initialize the Docker network
    pub async fn init(&self) -> NetworkResult<()> {
        // Check if network exists
        match self.docker.inspect_network::<String>(&self.config.network_name, None).await {
            Ok(_) => {
                info!("Docker network {} already exists", self.config.network_name);
                return Ok(());
            }
            Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. }) => {
                // Network doesn't exist, create it
            }
            Err(e) => {
                return Err(NetworkError::Cni(format!("Failed to inspect network: {}", e)));
            }
        }

        // Create the network
        let ipam_config = IpamConfig {
            subnet: Some(self.config.subnet.to_string()),
            gateway: Some(self.config.gateway.to_string()),
            ..Default::default()
        };

        let ipam = Ipam {
            driver: Some("default".to_string()),
            config: Some(vec![ipam_config]),
            ..Default::default()
        };

        let options = CreateNetworkOptions {
            name: self.config.network_name.clone(),
            driver: "bridge".to_string(),
            ipam,
            ..Default::default()
        };

        self.docker
            .create_network(options)
            .await
            .map_err(|e| NetworkError::Cni(format!("Failed to create network: {}", e)))?;

        info!(
            "Created Docker network {} with subnet {}",
            self.config.network_name, self.config.subnet
        );

        Ok(())
    }

    /// Allocate an IP for a container
    fn allocate_ip(&self, container_id: &str) -> NetworkResult<Ipv4Addr> {
        let mut allocations = self.allocations.write().unwrap();

        // Check if already allocated
        if let Some(&ip) = allocations.get(container_id) {
            return Ok(ip);
        }

        let mut next = self.next_ip.write().unwrap();
        let end_ip = u32::from(self.config.subnet.broadcast());

        // Find next available IP
        while *next < end_ip {
            let ip = Ipv4Addr::from(*next);
            *next += 1;

            // Skip gateway and broadcast
            if ip == self.config.gateway || ip == self.config.subnet.broadcast() {
                continue;
            }

            // Check if IP is already in use
            if !allocations.values().any(|&v| v == ip) {
                allocations.insert(container_id.to_string(), ip);
                info!("Allocated IP {} to container {}", ip, container_id);
                return Ok(ip);
            }
        }

        Err(NetworkError::Cni("No available IPs in subnet".to_string()))
    }

    /// Release an IP allocation
    fn release_ip(&self, container_id: &str) -> Option<Ipv4Addr> {
        let mut allocations = self.allocations.write().unwrap();
        let ip = allocations.remove(container_id);
        if let Some(ip) = ip {
            info!("Released IP {} from container {}", ip, container_id);
        }
        ip
    }

    /// Add a container to the network
    pub async fn add(&self, container_id: &str, docker_container_id: &str) -> NetworkResult<CniResult> {
        // Allocate IP
        let ip = self.allocate_ip(container_id)?;

        // Connect container to network with specific IP
        let endpoint_config = EndpointSettings {
            ipam_config: Some(EndpointIpamConfig {
                ipv4_address: Some(ip.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let connect_options = ConnectNetworkOptions {
            container: docker_container_id.to_string(),
            endpoint_config,
        };

        self.docker
            .connect_network(&self.config.network_name, connect_options)
            .await
            .map_err(|e| {
                // Release IP on failure
                self.release_ip(container_id);
                NetworkError::Cni(format!("Failed to connect container to network: {}", e))
            })?;

        let ip_cidr = format!("{}/{}", ip, self.config.subnet.prefix());

        debug!(
            "Added container {} ({}) with IP {} to network {}",
            container_id, docker_container_id, ip, self.config.network_name
        );

        Ok(CniResult {
            ips: vec![CniIp {
                address: ip_cidr,
                gateway: Some(self.config.gateway.to_string()),
                interface: Some(0),
            }],
            routes: vec![CniRoute {
                dst: "0.0.0.0/0".to_string(),
                gw: Some(self.config.gateway.to_string()),
            }],
            dns: CniDns {
                nameservers: vec!["10.43.0.10".to_string()],
                domain: Some("cluster.local".to_string()),
                search: vec![
                    "default.svc.cluster.local".to_string(),
                    "svc.cluster.local".to_string(),
                    "cluster.local".to_string(),
                ],
                options: vec![],
            },
        })
    }

    /// Remove a container from the network
    pub async fn del(&self, container_id: &str, docker_container_id: &str) -> NetworkResult<()> {
        // Disconnect from network
        let disconnect_options = DisconnectNetworkOptions {
            container: docker_container_id.to_string(),
            force: true,
        };

        if let Err(e) = self
            .docker
            .disconnect_network(&self.config.network_name, disconnect_options)
            .await
        {
            warn!(
                "Failed to disconnect container {} from network: {}",
                docker_container_id, e
            );
        }

        // Release IP
        self.release_ip(container_id);

        debug!("Removed container {} from network", container_id);
        Ok(())
    }

    /// Get allocated IP for a container
    pub fn get_ip(&self, container_id: &str) -> Option<Ipv4Addr> {
        let allocations = self.allocations.read().unwrap();
        allocations.get(container_id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_bridge_config_default() {
        let config = DockerBridgeConfig::default();
        assert_eq!(config.network_name, "k1s");
        assert_eq!(config.gateway, Ipv4Addr::new(10, 42, 0, 1));
    }
}
