//! P2P networking for k1s cluster nodes
//!
//! Implements a P2P mesh network using libp2p with:
//! - mDNS for local node discovery
//! - Kademlia DHT for distributed node registry
//! - GossipSub for state propagation
//! - Request-Response for direct node communication

pub mod behaviour;
pub mod cluster;
pub mod message;
pub mod node;

pub use cluster::P2pCluster;
pub use node::P2pNode;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum P2pError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("No peers available")]
    NoPeers,

    #[error("Timeout")]
    Timeout,
}

pub type P2pResult<T> = Result<T, P2pError>;
