//! iptables-based kube-proxy

use std::process::Command;

use async_trait::async_trait;
use k1s_types::Service;
use tracing::{info, warn};

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
    async fn sync_service(&self, service: &Service) -> NetworkResult<()> {
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

        // Add rules for each port
        for port in &spec.ports {
            // Jump to service chain from K1S-SERVICES
            self.run_iptables(&[
                "-t", "nat", "-A", "K1S-SERVICES",
                "-d", cluster_ip,
                "-p", &format!("{:?}", port.protocol.unwrap_or_default()).to_lowercase(),
                "--dport", &port.port.to_string(),
                "-j", &chain_name,
            ])?;

            // TODO: Add endpoint rules (DNAT to pod IPs)
        }

        info!("Synced service {}/{}", service.metadata.effective_namespace(), service.metadata.name);
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
