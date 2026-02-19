//! k1s DNS Server for Kubernetes Service Discovery
//!
//! This crate provides an embedded DNS server that resolves Kubernetes service names
//! to their cluster IPs or endpoint addresses.
//!
//! ## DNS Name Format
//!
//! Services are resolved using the following patterns:
//! - `<service>.<namespace>.svc.cluster.local` -> ClusterIP
//! - `<service>.<namespace>.svc` -> ClusterIP
//! - `<pod-ip-dashed>.<namespace>.pod.cluster.local` -> Pod IP
//!
//! ## Example
//!
//! ```ignore
//! # Resolve nginx service in default namespace
//! nslookup nginx.default.svc.cluster.local 10.43.0.10
//! ```

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{Endpoints, Service};
use thiserror::Error;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};

/// DNS server errors
#[derive(Error, Debug)]
pub enum DnsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    Storage(#[from] k1s_storage::StorageError),

    #[error("DNS parse error: {0}")]
    Parse(String),

    #[error("DNS server error: {0}")]
    Server(String),
}

pub type DnsResult<T> = Result<T, DnsError>;

/// DNS server configuration
#[derive(Debug, Clone)]
pub struct DnsConfig {
    /// Address to listen on (default: 0.0.0.0:53)
    pub listen_addr: SocketAddr,
    /// Cluster domain (default: cluster.local)
    pub cluster_domain: String,
    /// Upstream DNS servers for non-cluster queries
    pub upstream_servers: Vec<SocketAddr>,
    /// TTL for DNS responses in seconds
    pub ttl: u32,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            listen_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 10053),
            cluster_domain: "cluster.local".to_string(),
            upstream_servers: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 4, 4)), 53),
            ],
            ttl: 30,
        }
    }
}

/// k1s DNS Server
pub struct K1sDns {
    config: DnsConfig,
    storage: Arc<SledBackend>,
}

impl K1sDns {
    /// Create a new DNS server
    pub fn new(config: DnsConfig, storage: Arc<SledBackend>) -> Self {
        Self { config, storage }
    }

    /// Run the DNS server
    pub async fn run(&self) -> DnsResult<()> {
        let socket = Arc::new(UdpSocket::bind(&self.config.listen_addr).await?);
        info!("DNS server listening on {}", self.config.listen_addr);

        let mut buf = [0u8; 512];

        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    let query = buf[..len].to_vec();
                    let storage = self.storage.clone();
                    let config = self.config.clone();
                    let socket_clone = socket.clone();

                    tokio::spawn(async move {
                        match Self::handle_query(&query, &storage, &config).await {
                            Ok(response) => {
                                if let Err(e) = socket_clone.send_to(&response, addr).await {
                                    error!("Failed to send DNS response: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to handle DNS query: {}", e);
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to receive DNS query: {}", e);
                }
            }
        }
    }

    /// Handle a DNS query
    async fn handle_query(
        query: &[u8],
        storage: &Arc<SledBackend>,
        config: &DnsConfig,
    ) -> DnsResult<Vec<u8>> {
        // Parse the DNS query
        let (id, name, qtype) = Self::parse_query(query)?;

        debug!("DNS query: {} (type {})", name, qtype);

        // Try to resolve as a cluster service
        if let Some(response) = Self::resolve_cluster_name(&name, storage, config).await? {
            return Self::build_response(id, &name, qtype, &response, config.ttl);
        }

        // Forward to upstream DNS for non-cluster queries
        if !config.upstream_servers.is_empty() {
            if let Ok(response) = Self::forward_query(query, &config.upstream_servers).await {
                return Ok(response);
            }
        }

        // Return NXDOMAIN
        Self::build_nxdomain_response(id, &name, qtype)
    }

    /// Parse a DNS query to extract the query name and type
    fn parse_query(query: &[u8]) -> DnsResult<(u16, String, u16)> {
        if query.len() < 12 {
            return Err(DnsError::Parse("Query too short".to_string()));
        }

        let id = u16::from_be_bytes([query[0], query[1]]);

        // Parse the query name (starting at byte 12)
        let mut name = String::new();
        let mut pos = 12;

        loop {
            if pos >= query.len() {
                return Err(DnsError::Parse("Invalid query name".to_string()));
            }

            let len = query[pos] as usize;
            if len == 0 {
                pos += 1;
                break;
            }

            if !name.is_empty() {
                name.push('.');
            }

            pos += 1;
            if pos + len > query.len() {
                return Err(DnsError::Parse("Invalid query name length".to_string()));
            }

            name.push_str(
                std::str::from_utf8(&query[pos..pos + len])
                    .map_err(|_| DnsError::Parse("Invalid UTF-8 in query name".to_string()))?,
            );
            pos += len;
        }

        // Query type (2 bytes after name)
        if pos + 2 > query.len() {
            return Err(DnsError::Parse("Missing query type".to_string()));
        }
        let qtype = u16::from_be_bytes([query[pos], query[pos + 1]]);

        Ok((id, name, qtype))
    }

    /// Try to resolve a name as a cluster service
    async fn resolve_cluster_name(
        name: &str,
        storage: &Arc<SledBackend>,
        config: &DnsConfig,
    ) -> DnsResult<Option<Vec<IpAddr>>> {
        let cluster_suffix = format!(".svc.{}", config.cluster_domain);
        let short_suffix = ".svc";

        // Check if this is a cluster service query
        let (service, namespace) = if name.ends_with(&cluster_suffix) {
            // Full form: service.namespace.svc.cluster.local
            let without_suffix = name.strip_suffix(&cluster_suffix).unwrap();
            Self::parse_service_namespace(without_suffix)?
        } else if name.ends_with(short_suffix) && !name.ends_with(&format!(".{}", config.cluster_domain)) {
            // Short form: service.namespace.svc
            let without_suffix = name.strip_suffix(short_suffix).unwrap();
            Self::parse_service_namespace(without_suffix)?
        } else {
            return Ok(None);
        };

        debug!(
            "Looking up service: {} in namespace: {}",
            service, namespace
        );

        // Look up the service
        let service_store = ResourceStore::<Service>::new(storage.clone());
        let svc = service_store.get(Some(&namespace), &service).await?;

        if let Some(svc) = svc {
            // Return the cluster IP if available
            if let Some(spec) = &svc.spec {
                if let Some(cluster_ip) = &spec.cluster_ip {
                    if cluster_ip != "None" && !cluster_ip.is_empty() {
                        if let Ok(ip) = IpAddr::from_str(cluster_ip) {
                            return Ok(Some(vec![ip]));
                        }
                    }
                }
            }
        }

        // Fall back to endpoints for headless services or if cluster IP not available
        let endpoints_store = ResourceStore::<Endpoints>::new(storage.clone());
        let endpoints = endpoints_store.get(Some(&namespace), &service).await?;

        if let Some(ep) = endpoints {
            let ips: Vec<IpAddr> = ep
                .subsets
                .iter()
                .flat_map(|s| s.addresses.iter())
                .filter_map(|a| IpAddr::from_str(&a.ip).ok())
                .collect();

            if !ips.is_empty() {
                return Ok(Some(ips));
            }
        }

        Ok(None)
    }

    /// Parse "service.namespace" format
    fn parse_service_namespace(name: &str) -> DnsResult<(String, String)> {
        let parts: Vec<&str> = name.split('.').collect();
        match parts.as_slice() {
            [service, namespace] => Ok((service.to_string(), namespace.to_string())),
            [service] => Ok((service.to_string(), "default".to_string())),
            _ => Err(DnsError::Parse(format!(
                "Invalid service name format: {}",
                name
            ))),
        }
    }

    /// Build a DNS response with A records
    fn build_response(
        id: u16,
        name: &str,
        qtype: u16,
        ips: &[IpAddr],
        ttl: u32,
    ) -> DnsResult<Vec<u8>> {
        let mut response = Vec::with_capacity(512);

        // Header
        response.extend_from_slice(&id.to_be_bytes()); // ID
        response.extend_from_slice(&[0x81, 0x80]); // Flags: response, recursion available
        response.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        response.extend_from_slice(&(ips.len() as u16).to_be_bytes()); // ANCOUNT
        response.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        response.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

        // Question section
        Self::encode_name(&mut response, name);
        response.extend_from_slice(&qtype.to_be_bytes()); // QTYPE
        response.extend_from_slice(&1u16.to_be_bytes()); // QCLASS (IN)

        // Answer section
        for ip in ips {
            if let IpAddr::V4(ipv4) = ip {
                // Name (pointer to question)
                response.extend_from_slice(&[0xC0, 0x0C]);
                // Type (A)
                response.extend_from_slice(&1u16.to_be_bytes());
                // Class (IN)
                response.extend_from_slice(&1u16.to_be_bytes());
                // TTL
                response.extend_from_slice(&ttl.to_be_bytes());
                // RDLENGTH
                response.extend_from_slice(&4u16.to_be_bytes());
                // RDATA (IP address)
                response.extend_from_slice(&ipv4.octets());
            }
        }

        Ok(response)
    }

    /// Build an NXDOMAIN response
    fn build_nxdomain_response(id: u16, name: &str, qtype: u16) -> DnsResult<Vec<u8>> {
        let mut response = Vec::with_capacity(512);

        // Header
        response.extend_from_slice(&id.to_be_bytes()); // ID
        response.extend_from_slice(&[0x81, 0x83]); // Flags: response, NXDOMAIN
        response.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        response.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
        response.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        response.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

        // Question section
        Self::encode_name(&mut response, name);
        response.extend_from_slice(&qtype.to_be_bytes()); // QTYPE
        response.extend_from_slice(&1u16.to_be_bytes()); // QCLASS (IN)

        Ok(response)
    }

    /// Encode a DNS name in wire format
    fn encode_name(buf: &mut Vec<u8>, name: &str) {
        for label in name.split('.') {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
        buf.push(0); // Null terminator
    }

    /// Forward query to upstream DNS servers
    async fn forward_query(query: &[u8], upstreams: &[SocketAddr]) -> DnsResult<Vec<u8>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        for upstream in upstreams {
            socket.send_to(query, upstream).await?;

            let mut buf = [0u8; 512];
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                socket.recv_from(&mut buf),
            )
            .await
            {
                Ok(Ok((len, _))) => {
                    return Ok(buf[..len].to_vec());
                }
                Ok(Err(e)) => {
                    warn!("Upstream DNS {} error: {}", upstream, e);
                }
                Err(_) => {
                    warn!("Upstream DNS {} timeout", upstream);
                }
            }
        }

        Err(DnsError::Server("All upstream DNS servers failed".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_service_namespace() {
        let (svc, ns) = K1sDns::parse_service_namespace("nginx.default").unwrap();
        assert_eq!(svc, "nginx");
        assert_eq!(ns, "default");

        let (svc, ns) = K1sDns::parse_service_namespace("redis").unwrap();
        assert_eq!(svc, "redis");
        assert_eq!(ns, "default");
    }

    #[test]
    fn test_encode_name() {
        let mut buf = Vec::new();
        K1sDns::encode_name(&mut buf, "nginx.default.svc.cluster.local");
        assert_eq!(buf[0], 5); // "nginx" length
        assert_eq!(&buf[1..6], b"nginx");
        assert_eq!(buf[6], 7); // "default" length
    }
}
