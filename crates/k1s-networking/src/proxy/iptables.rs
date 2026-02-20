//! iptables-based kube-proxy

use std::process::Command;

use async_trait::async_trait;
use k1s_types::{Endpoints, IntOrString, Service, ServiceType};
use tracing::{debug, info, warn};

use super::ServiceProxy;
use crate::{NetworkError, NetworkResult};

/// iptables-based service proxy
pub struct IptablesProxy {
    cluster_cidr: String,
    service_cidr: String,
}

impl IptablesProxy {
    pub fn new(cluster_cidr: &str, service_cidr: &str) -> Self {
        Self {
            cluster_cidr: cluster_cidr.to_string(),
            service_cidr: service_cidr.to_string(),
        }
    }

    /// Initialize iptables chains
    pub fn init(&self) -> NetworkResult<()> {
        // Create custom chains
        self.run_iptables(&["-t", "nat", "-N", "K1S-SERVICES"])?;
        self.run_iptables(&["-t", "nat", "-N", "K1S-NODEPORTS"])?;
        self.run_iptables(&["-t", "nat", "-N", "K1S-POSTROUTING"])?;

        // Add jumps from built-in chains
        self.run_iptables(&[
            "-t", "nat", "-A", "PREROUTING",
            "-j", "K1S-SERVICES",
        ])?;
        self.run_iptables(&[
            "-t", "nat", "-A", "OUTPUT",
            "-j", "K1S-SERVICES",
        ])?;
        self.run_iptables(&[
            "-t", "nat", "-A", "POSTROUTING",
            "-j", "K1S-POSTROUTING",
        ])?;

        // Add masquerade for pod traffic
        self.run_iptables(&[
            "-t", "nat", "-A", "K1S-POSTROUTING",
            "-s", &self.cluster_cidr,
            "!", "-d", &self.cluster_cidr,
            "-j", "MASQUERADE",
        ])?;

        info!("Initialized iptables chains");
        Ok(())
    }

    fn run_iptables(&self, args: &[&str]) -> NetworkResult<()> {
        let output = Command::new("iptables")
            .args(args)
            .output()
            .map_err(|e| NetworkError::Proxy(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "already exists" errors
            if !stderr.contains("already exists") {
                warn!("iptables command failed: {}", stderr);
            }
        }

        Ok(())
    }

    fn service_chain_name(&self, service: &Service) -> String {
        format!(
            "K1S-SVC-{}",
            &service.metadata.uid[..8].to_uppercase()
        )
    }
}

#[async_trait]
impl ServiceProxy for IptablesProxy {
    async fn sync_service(&self, service: &Service, endpoints: Option<&Endpoints>) -> NetworkResult<()> {
        let spec = match &service.spec {
            Some(s) => s,
            None => return Ok(()),
        };

        let cluster_ip = match &spec.cluster_ip {
            Some(ip) if ip != "None" => ip,
            _ => return Ok(()), // Headless service
        };

        let chain_name = self.service_chain_name(service);

        // Create service chain
        let _ = self.run_iptables(&["-t", "nat", "-N", &chain_name]);

        // Flush existing rules
        self.run_iptables(&["-t", "nat", "-F", &chain_name])?;

        // Collect endpoint addresses
        let endpoint_addresses: Vec<(String, i32)> = endpoints
            .map(|ep| {
                ep.subsets.iter().flat_map(|subset| {
                    let ports: Vec<i32> = subset.ports.iter()
                        .map(|p| p.port)
                        .collect();
                    subset.addresses.iter()
                        .map(|addr| addr.ip.clone())
                        .flat_map(|ip| {
                            ports.iter().map(move |port| (ip.clone(), *port))
                        })
                        .collect::<Vec<_>>()
                }).collect()
            })
            .unwrap_or_default();

        // Add rules for each port
        for port in &spec.ports {
            let protocol = format!("{:?}", port.protocol.clone().unwrap_or_default()).to_lowercase();
            let target_port = port.target_port.as_ref()
                .map(|tp| match tp {
                    IntOrString::Int(i) => *i,
                    IntOrString::String(s) => s.parse::<i32>().unwrap_or(port.port),
                })
                .unwrap_or(port.port);

            // Jump to service chain from K1S-SERVICES
            self.run_iptables(&[
                "-t", "nat", "-A", "K1S-SERVICES",
                "-d", cluster_ip,
                "-p", &protocol,
                "--dport", &port.port.to_string(),
                "-j", &chain_name,
            ])?;

            // Get endpoints for this port
            let port_endpoints: Vec<&(String, i32)> = endpoint_addresses.iter()
                .filter(|(_, p)| *p == target_port)
                .collect();

            if port_endpoints.is_empty() {
                debug!("No endpoints for service {}/{} port {}",
                    service.metadata.effective_namespace(), service.metadata.name, port.port);
                continue;
            }

            // Create endpoint chains and add DNAT rules with probability-based load balancing
            let num_endpoints = port_endpoints.len();
            for (i, (ip, ep_port)) in port_endpoints.iter().enumerate() {
                let ep_chain = format!("{}-{}", chain_name, i);

                // Create endpoint chain
                let _ = self.run_iptables(&["-t", "nat", "-N", &ep_chain]);
                self.run_iptables(&["-t", "nat", "-F", &ep_chain])?;

                // Add DNAT rule in endpoint chain
                self.run_iptables(&[
                    "-t", "nat", "-A", &ep_chain,
                    "-j", "DNAT",
                    "--to-destination", &format!("{}:{}", ip, ep_port),
                ])?;

                // Add jump to endpoint chain from service chain with probability
                // For fair load balancing, use 1/remaining_endpoints probability
                let remaining = num_endpoints - i;
                if remaining > 1 {
                    // Use statistic module for random selection
                    self.run_iptables(&[
                        "-t", "nat", "-A", &chain_name,
                        "-m", "statistic",
                        "--mode", "random",
                        "--probability", &format!("{:.10}", 1.0 / remaining as f64),
                        "-j", &ep_chain,
                    ])?;
                } else {
                    // Last endpoint - always select
                    self.run_iptables(&[
                        "-t", "nat", "-A", &chain_name,
                        "-j", &ep_chain,
                    ])?;
                }
            }
        }

        // Handle NodePort if set
        if let Some(node_port_type) = &spec.r#type {
            if matches!(node_port_type, ServiceType::NodePort | ServiceType::LoadBalancer) {
                for port in &spec.ports {
                    if let Some(node_port) = port.node_port {
                        let protocol = format!("{:?}", port.protocol.clone().unwrap_or_default()).to_lowercase();
                        // Add NodePort rule
                        self.run_iptables(&[
                            "-t", "nat", "-A", "K1S-NODEPORTS",
                            "-p", &protocol,
                            "--dport", &node_port.to_string(),
                            "-j", &chain_name,
                        ])?;
                    }
                }
            }
        }

        info!("Synced service {}/{} with {} endpoints",
            service.metadata.effective_namespace(), service.metadata.name, endpoint_addresses.len());
        Ok(())
    }

    async fn remove_service(&self, service: &Service) -> NetworkResult<()> {
        let chain_name = self.service_chain_name(service);

        // Remove references to the chain
        // (In practice, we'd need to remove specific rules)

        // Flush and delete the chain
        let _ = self.run_iptables(&["-t", "nat", "-F", &chain_name]);
        let _ = self.run_iptables(&["-t", "nat", "-X", &chain_name]);

        info!("Removed service {}/{}", service.metadata.effective_namespace(), service.metadata.name);
        Ok(())
    }

    async fn sync_all(&self) -> NetworkResult<()> {
        // TODO: Sync all services from storage
        Ok(())
    }
}
