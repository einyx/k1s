//! Rust-native CNI bridge plugin
//!
//! Implements a simple bridge network for pod connectivity.

use std::collections::HashMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, RwLock};

use ipnetwork::Ipv4Network;
use tracing::{debug, info, warn};

use super::{CniDns, CniIp, CniResult, CniRoute};
use crate::{NetworkError, NetworkResult};

/// Bridge CNI plugin configuration
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// Name of the bridge interface (e.g., "k1s0")
    pub bridge_name: String,
    /// Subnet for pod IPs (e.g., "10.42.0.0/24")
    pub subnet: Ipv4Network,
    /// Gateway IP (usually .1 of subnet)
    pub gateway: Ipv4Addr,
    /// MTU for interfaces
    pub mtu: u32,
    /// Enable IP masquerading
    pub masquerade: bool,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        let subnet = "10.42.0.0/24".parse().unwrap();
        Self {
            bridge_name: "k1s0".to_string(),
            subnet,
            gateway: "10.42.0.1".parse().unwrap(),
            mtu: 1500,
            masquerade: true,
        }
    }
}

/// IPAM - IP Address Management
#[derive(Debug)]
pub struct Ipam {
    subnet: Ipv4Network,
    gateway: Ipv4Addr,
    /// Map of container_id -> allocated IP
    allocations: RwLock<HashMap<String, Ipv4Addr>>,
    /// Next IP to try allocating
    next_ip: RwLock<u32>,
}

impl Ipam {
    pub fn new(subnet: Ipv4Network, gateway: Ipv4Addr) -> Self {
        // Start allocating from .2 (after gateway)
        let start_ip = u32::from(subnet.ip()) + 2;
        Self {
            subnet,
            gateway,
            allocations: RwLock::new(HashMap::new()),
            next_ip: RwLock::new(start_ip),
        }
    }

    /// Allocate an IP for a container
    pub fn allocate(&self, container_id: &str) -> NetworkResult<Ipv4Addr> {
        let mut allocations = self.allocations.write().unwrap();

        // Check if already allocated
        if let Some(&ip) = allocations.get(container_id) {
            return Ok(ip);
        }

        let mut next = self.next_ip.write().unwrap();
        let end_ip = u32::from(self.subnet.broadcast());

        // Find next available IP
        while *next < end_ip {
            let ip = Ipv4Addr::from(*next);
            *next += 1;

            // Skip gateway and broadcast
            if ip == self.gateway || ip == self.subnet.broadcast() {
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
    pub fn release(&self, container_id: &str) -> Option<Ipv4Addr> {
        let mut allocations = self.allocations.write().unwrap();
        let ip = allocations.remove(container_id);
        if let Some(ip) = ip {
            info!("Released IP {} from container {}", ip, container_id);
        }
        ip
    }

    /// Get allocated IP for a container
    pub fn get(&self, container_id: &str) -> Option<Ipv4Addr> {
        let allocations = self.allocations.read().unwrap();
        allocations.get(container_id).copied()
    }
}

/// Bridge CNI plugin
pub struct BridgeCni {
    config: BridgeConfig,
    ipam: Arc<Ipam>,
}

impl BridgeCni {
    pub fn new(config: BridgeConfig) -> Self {
        let ipam = Arc::new(Ipam::new(config.subnet, config.gateway));
        Self { config, ipam }
    }

    /// Initialize the bridge (called once at startup)
    pub fn init(&self) -> NetworkResult<()> {
        // Check if we're on Linux
        if cfg!(not(target_os = "linux")) {
            warn!("CNI bridge plugin only works on Linux, skipping initialization");
            return Ok(());
        }

        // Create bridge if it doesn't exist
        if !self.bridge_exists()? {
            self.create_bridge()?;
        }

        // Set up masquerading if enabled
        if self.config.masquerade {
            self.setup_masquerade()?;
        }

        info!("Initialized CNI bridge {} with subnet {}",
              self.config.bridge_name, self.config.subnet);
        Ok(())
    }

    /// Add a container to the network
    pub fn add(
        &self,
        container_id: &str,
        netns_path: &str,
        ifname: &str,
    ) -> NetworkResult<CniResult> {
        if cfg!(not(target_os = "linux")) {
            // On non-Linux, return a mock result
            return Ok(self.mock_add(container_id));
        }

        // Allocate IP
        let ip = self.ipam.allocate(container_id)?;
        let ip_cidr = format!("{}/{}", ip, self.config.subnet.prefix());

        // Create veth pair
        let host_veth = format!("veth{}", &container_id[..8.min(container_id.len())]);
        let container_veth = ifname;

        self.create_veth_pair(&host_veth, container_veth, netns_path)?;

        // Attach host end to bridge
        self.attach_to_bridge(&host_veth)?;

        // Configure container interface
        self.configure_container_interface(netns_path, container_veth, &ip_cidr)?;

        // Set up default route in container
        self.setup_container_route(netns_path, container_veth)?;

        // Bring up interfaces
        self.bring_up_interface(&host_veth)?;
        self.bring_up_interface_in_netns(netns_path, container_veth)?;

        debug!("Added container {} with IP {} to bridge {}",
               container_id, ip, self.config.bridge_name);

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
                nameservers: vec!["10.43.0.10".to_string()], // kube-dns
                domain: Some("cluster.local".to_string()),
                search: vec!["default.svc.cluster.local".to_string(),
                            "svc.cluster.local".to_string(),
                            "cluster.local".to_string()],
                options: vec![],
            },
        })
    }

    /// Remove a container from the network
    pub fn del(&self, container_id: &str, netns_path: &str, _ifname: &str) -> NetworkResult<()> {
        if cfg!(not(target_os = "linux")) {
            self.ipam.release(container_id);
            return Ok(());
        }

        // Delete veth pair (deleting one end removes both)
        let host_veth = format!("veth{}", &container_id[..8.min(container_id.len())]);
        let _ = self.delete_interface(&host_veth);

        // Release IP
        self.ipam.release(container_id);

        debug!("Removed container {} from network", container_id);
        Ok(())
    }

    /// Check if bridge exists
    fn bridge_exists(&self) -> NetworkResult<bool> {
        let path = format!("/sys/class/net/{}/bridge", self.config.bridge_name);
        Ok(Path::new(&path).exists())
    }

    /// Create the bridge interface
    fn create_bridge(&self) -> NetworkResult<()> {
        // ip link add <bridge> type bridge
        run_ip(&["link", "add", &self.config.bridge_name, "type", "bridge"])?;

        // ip addr add <gateway>/<prefix> dev <bridge>
        let addr = format!("{}/{}", self.config.gateway, self.config.subnet.prefix());
        run_ip(&["addr", "add", &addr, "dev", &self.config.bridge_name])?;

        // ip link set <bridge> up
        run_ip(&["link", "set", &self.config.bridge_name, "up"])?;

        info!("Created bridge {} with gateway {}", self.config.bridge_name, self.config.gateway);
        Ok(())
    }

    /// Create veth pair
    fn create_veth_pair(&self, host_veth: &str, container_veth: &str, netns_path: &str) -> NetworkResult<()> {
        // Delete existing veth if it exists
        let _ = run_ip(&["link", "delete", host_veth]);

        // ip link add <host_veth> type veth peer name <container_veth>
        run_ip(&["link", "add", host_veth, "type", "veth", "peer", "name", container_veth])?;

        // Set MTU
        let mtu = self.config.mtu.to_string();
        run_ip(&["link", "set", host_veth, "mtu", &mtu])?;
        run_ip(&["link", "set", container_veth, "mtu", &mtu])?;

        // Move container end to network namespace
        // ip link set <container_veth> netns <netns_path>
        run_ip(&["link", "set", container_veth, "netns", netns_path])?;

        Ok(())
    }

    /// Attach interface to bridge
    fn attach_to_bridge(&self, ifname: &str) -> NetworkResult<()> {
        // ip link set <ifname> master <bridge>
        run_ip(&["link", "set", ifname, "master", &self.config.bridge_name])?;
        Ok(())
    }

    /// Configure container interface with IP
    fn configure_container_interface(&self, netns_path: &str, ifname: &str, ip_cidr: &str) -> NetworkResult<()> {
        // ip netns exec <netns> ip addr add <ip_cidr> dev <ifname>
        run_ip_netns(netns_path, &["addr", "add", ip_cidr, "dev", ifname])?;
        Ok(())
    }

    /// Set up default route in container
    fn setup_container_route(&self, netns_path: &str, ifname: &str) -> NetworkResult<()> {
        let gw = self.config.gateway.to_string();
        // ip netns exec <netns> ip route add default via <gateway> dev <ifname>
        run_ip_netns(netns_path, &["route", "add", "default", "via", &gw, "dev", ifname])?;
        Ok(())
    }

    /// Bring up interface
    fn bring_up_interface(&self, ifname: &str) -> NetworkResult<()> {
        run_ip(&["link", "set", ifname, "up"])?;
        Ok(())
    }

    /// Bring up interface in network namespace
    fn bring_up_interface_in_netns(&self, netns_path: &str, ifname: &str) -> NetworkResult<()> {
        run_ip_netns(netns_path, &["link", "set", ifname, "up"])?;
        // Also bring up loopback
        let _ = run_ip_netns(netns_path, &["link", "set", "lo", "up"]);
        Ok(())
    }

    /// Delete interface
    fn delete_interface(&self, ifname: &str) -> NetworkResult<()> {
        run_ip(&["link", "delete", ifname])?;
        Ok(())
    }

    /// Set up IP masquerading
    fn setup_masquerade(&self) -> NetworkResult<()> {
        let subnet = self.config.subnet.to_string();

        // Enable IP forwarding
        let _ = fs::write("/proc/sys/net/ipv4/ip_forward", "1");

        // iptables -t nat -A POSTROUTING -s <subnet> ! -d <subnet> -j MASQUERADE
        let output = Command::new("iptables")
            .args([
                "-t", "nat", "-C", "POSTROUTING",
                "-s", &subnet, "!", "-d", &subnet,
                "-j", "MASQUERADE"
            ])
            .output();

        // Only add if rule doesn't exist
        if output.map(|o| !o.status.success()).unwrap_or(true) {
            let _ = Command::new("iptables")
                .args([
                    "-t", "nat", "-A", "POSTROUTING",
                    "-s", &subnet, "!", "-d", &subnet,
                    "-j", "MASQUERADE"
                ])
                .output();
        }

        info!("Set up IP masquerading for {}", subnet);
        Ok(())
    }

    /// Mock add for non-Linux platforms
    fn mock_add(&self, container_id: &str) -> CniResult {
        // Just allocate an IP for tracking
        let ip = self.ipam.allocate(container_id).unwrap_or_else(|_| {
            // Fallback to a deterministic IP based on container ID
            let hash = container_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let last_octet = (hash % 253) as u8 + 2;
            Ipv4Addr::new(10, 42, 0, last_octet)
        });

        CniResult {
            ips: vec![CniIp {
                address: format!("{}/24", ip),
                gateway: Some(self.config.gateway.to_string()),
                interface: Some(0),
            }],
            routes: vec![CniRoute {
                dst: "0.0.0.0/0".to_string(),
                gw: Some(self.config.gateway.to_string()),
            }],
            dns: CniDns::default(),
        }
    }

    /// Get the IPAM instance
    pub fn ipam(&self) -> &Arc<Ipam> {
        &self.ipam
    }
}

/// Run ip command
fn run_ip(args: &[&str]) -> NetworkResult<()> {
    let output = Command::new("ip")
        .args(args)
        .output()
        .map_err(|e| NetworkError::Cni(format!("Failed to run ip command: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetworkError::Cni(format!(
            "ip {} failed: {}",
            args.join(" "),
            stderr
        )));
    }

    Ok(())
}

/// Run ip command in a network namespace
fn run_ip_netns(netns_path: &str, args: &[&str]) -> NetworkResult<()> {
    // Use nsenter to run in the network namespace
    let mut full_args = vec![
        "--net".to_string(),
        format!("--net={}", netns_path),
        "ip".to_string(),
    ];
    full_args.extend(args.iter().map(|s| s.to_string()));

    let output = Command::new("nsenter")
        .arg("--net")
        .arg(netns_path)
        .arg("ip")
        .args(args)
        .output()
        .map_err(|e| NetworkError::Cni(format!("Failed to run nsenter: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(NetworkError::Cni(format!(
            "ip {} in netns failed: {}",
            args.join(" "),
            stderr
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipam_allocation() {
        let subnet = "10.42.0.0/24".parse().unwrap();
        let gateway = "10.42.0.1".parse().unwrap();
        let ipam = Ipam::new(subnet, gateway);

        // First allocation should be .2
        let ip1 = ipam.allocate("container1").unwrap();
        assert_eq!(ip1, Ipv4Addr::new(10, 42, 0, 2));

        // Second allocation should be .3
        let ip2 = ipam.allocate("container2").unwrap();
        assert_eq!(ip2, Ipv4Addr::new(10, 42, 0, 3));

        // Same container should get same IP
        let ip1_again = ipam.allocate("container1").unwrap();
        assert_eq!(ip1_again, ip1);

        // Release and reallocate
        ipam.release("container1");
        assert!(ipam.get("container1").is_none());
    }

    #[test]
    fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.bridge_name, "k1s0");
        assert_eq!(config.gateway, Ipv4Addr::new(10, 42, 0, 1));
        assert_eq!(config.mtu, 1500);
    }
}
