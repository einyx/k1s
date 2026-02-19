//! P2P message types

use serde::{Deserialize, Serialize};

/// Topics for gossip subscription
pub mod topics {
    pub const NODES: &str = "k1s/nodes";
    pub const PODS: &str = "k1s/pods";
    pub const SERVICES: &str = "k1s/services";
    pub const EVENTS: &str = "k1s/events";
}

/// Message types exchanged between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    /// Node heartbeat/announcement
    NodeHeartbeat(NodeInfo),

    /// Resource update notification
    ResourceUpdate(ResourceUpdate),

    /// Request to sync resources
    SyncRequest(SyncRequest),

    /// Response to sync request
    SyncResponse(SyncResponse),

    /// Leader election
    LeaderElection(LeaderElection),

    /// Direct request to another node
    Request(NodeRequest),

    /// Response to a request
    Response(NodeResponse),
}

/// Information about a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_name: String,
    pub peer_id: String,
    pub api_address: String,
    pub kubelet_port: u16,
    pub is_control_plane: bool,
    pub is_worker: bool,
    pub timestamp: i64,
    pub capacity: NodeCapacity,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeCapacity {
    pub cpu_cores: u32,
    pub memory_bytes: u64,
    pub pods: u32,
}

/// Resource update notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdate {
    pub resource_type: String,
    pub namespace: Option<String>,
    pub name: String,
    pub action: ResourceAction,
    pub revision: i64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResourceAction {
    Created,
    Updated,
    Deleted,
}

/// Sync request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub resource_type: String,
    pub namespace: Option<String>,
    pub since_revision: i64,
}

/// Sync response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub resource_type: String,
    pub namespace: Option<String>,
    pub resources: Vec<Vec<u8>>,
    pub current_revision: i64,
}

/// Leader election message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderElection {
    pub candidate_id: String,
    pub term: u64,
    pub vote_request: bool,
    pub vote_granted: bool,
}

/// Direct node request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRequest {
    pub request_id: String,
    pub request_type: RequestType,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    GetPodLogs { pod_name: String, container: Option<String> },
    ExecInPod { pod_name: String, container: Option<String>, command: Vec<String> },
    GetNodeMetrics,
    SchedulePod { pod_data: Vec<u8> },
}

/// Node response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResponse {
    pub request_id: String,
    pub success: bool,
    pub payload: Vec<u8>,
    pub error: Option<String>,
}
