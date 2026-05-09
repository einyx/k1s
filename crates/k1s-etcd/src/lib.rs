//! k1s-etcd: Embedded Raft-based distributed KV store
//!
//! Provides etcd-compatible API using Raft consensus, embedded directly in k1s.
//! No external dependencies - everything runs in the k1s binary.

pub mod store;
pub mod raft_node;
pub mod server;
pub mod client;

use std::path::PathBuf;
use std::net::SocketAddr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EtcdError {
    #[error("Raft error: {0}")]
    Raft(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Not leader")]
    NotLeader,

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, EtcdError>;

/// Embedded etcd configuration
#[derive(Debug, Clone)]
pub struct EtcdConfig {
    /// Node ID (1, 2, 3, etc.)
    pub node_id: u64,

    /// Data directory for Raft logs and state
    pub data_dir: PathBuf,

    /// Address to bind for client requests (etcd API)
    pub client_addr: SocketAddr,

    /// Address to bind for peer communication (Raft)
    pub peer_addr: SocketAddr,

    /// Initial cluster members (for bootstrap)
    /// Format: "id=http://peer_addr,id=http://peer_addr"
    pub initial_cluster: Vec<(u64, String)>,

    /// Join existing cluster (vs bootstrap new)
    pub join: bool,
}

impl Default for EtcdConfig {
    fn default() -> Self {
        Self {
            node_id: 1,
            data_dir: PathBuf::from("/var/lib/k1s/etcd"),
            client_addr: "127.0.0.1:2379".parse().unwrap(),
            peer_addr: "127.0.0.1:2380".parse().unwrap(),
            initial_cluster: vec![
                (1, "http://127.0.0.1:2380".to_string()),
            ],
            join: false,
        }
    }
}

/// Embedded etcd instance
pub struct EmbeddedEtcd {
    config: EtcdConfig,
    raft_node: raft_node::RaftNode,
    server: server::EtcdServer,
}

impl EmbeddedEtcd {
    /// Create new embedded etcd instance
    pub async fn new(config: EtcdConfig) -> Result<Self> {
        // Create data directory
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| EtcdError::Storage(e.to_string()))?;

        // Initialize Raft node
        let raft_node = raft_node::RaftNode::new(&config).await?;

        // Create etcd API server
        let server = server::EtcdServer::new(
            config.client_addr,
            raft_node.clone(),
        );

        tracing::info!(
            "Embedded etcd initialized: node_id={}, client={}, peer={}",
            config.node_id,
            config.client_addr,
            config.peer_addr
        );

        Ok(Self {
            config,
            raft_node,
            server,
        })
    }

    /// Start the embedded etcd server
    pub async fn start(&self) -> Result<()> {
        // Start Raft consensus
        self.raft_node.start().await?;

        // Start etcd API server
        self.server.start().await?;

        tracing::info!("Embedded etcd started on {}", self.config.client_addr);
        Ok(())
    }

    /// Get client for local access
    pub fn client(&self) -> client::EtcdClient {
        client::EtcdClient::new(format!("http://{}", self.config.client_addr))
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        self.raft_node.is_leader().await
    }

    /// Get cluster status
    pub async fn cluster_status(&self) -> ClusterStatus {
        ClusterStatus {
            node_id: self.config.node_id,
            is_leader: self.is_leader().await,
            members: self.raft_node.members().await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClusterStatus {
    pub node_id: u64,
    pub is_leader: bool,
    pub members: Vec<Member>,
}

#[derive(Debug, Clone)]
pub struct Member {
    pub id: u64,
    pub peer_url: String,
    pub client_url: String,
}
