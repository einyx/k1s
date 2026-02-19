//! P2P cluster management

use std::collections::HashMap;
use std::sync::Arc;

use libp2p::PeerId;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::message::{topics, NodeInfo, P2pMessage, ResourceAction, ResourceUpdate};
use crate::node::{P2pConfig, P2pEvent, P2pNode};
use crate::P2pResult;

/// P2P cluster state
pub struct P2pCluster {
    node: P2pNode,
    leader: Arc<RwLock<Option<PeerId>>>,
    members: Arc<RwLock<HashMap<PeerId, ClusterMember>>>,
}

/// Cluster member info
#[derive(Debug, Clone)]
pub struct ClusterMember {
    pub info: NodeInfo,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub role: MemberRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    ControlPlane,
    Worker,
    Combined,
}

impl P2pCluster {
    /// Create a new P2P cluster
    pub async fn new(config: P2pConfig) -> P2pResult<Self> {
        let node = P2pNode::new(config).await?;

        Ok(Self {
            node,
            leader: Arc::new(RwLock::new(None)),
            members: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start the cluster
    pub async fn start(&mut self) -> P2pResult<()> {
        self.node.start().await?;
        info!("P2P cluster started");
        Ok(())
    }

    /// Run the cluster event loop
    pub async fn run(&mut self) -> P2pResult<()> {
        loop {
            tokio::select! {
                result = self.node.run() => {
                    if let Err(e) = result {
                        warn!("P2P node error: {}", e);
                    }
                }
            }
        }
    }

    /// Get current cluster leader
    pub async fn leader(&self) -> Option<PeerId> {
        *self.leader.read().await
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        let leader = self.leader.read().await;
        leader.map(|l| l == self.node.local_peer_id()).unwrap_or(false)
    }

    /// Get all cluster members
    pub async fn members(&self) -> HashMap<PeerId, ClusterMember> {
        self.members.read().await.clone()
    }

    /// Get control plane members
    pub async fn control_plane_members(&self) -> Vec<ClusterMember> {
        self.members
            .read()
            .await
            .values()
            .filter(|m| {
                m.role == MemberRole::ControlPlane || m.role == MemberRole::Combined
            })
            .cloned()
            .collect()
    }

    /// Get worker members
    pub async fn worker_members(&self) -> Vec<ClusterMember> {
        self.members
            .read()
            .await
            .values()
            .filter(|m| m.role == MemberRole::Worker || m.role == MemberRole::Combined)
            .cloned()
            .collect()
    }

    /// Broadcast a resource update to the cluster
    pub fn broadcast_resource_update(
        &mut self,
        resource_type: &str,
        namespace: Option<&str>,
        name: &str,
        action: ResourceAction,
        revision: i64,
        data: Vec<u8>,
    ) -> P2pResult<()> {
        let update = ResourceUpdate {
            resource_type: resource_type.to_string(),
            namespace: namespace.map(String::from),
            name: name.to_string(),
            action,
            revision,
            data,
        };

        let topic = match resource_type {
            "pods" | "Pod" => topics::PODS,
            "services" | "Service" => topics::SERVICES,
            _ => topics::EVENTS,
        };

        self.node.publish(topic, &P2pMessage::ResourceUpdate(update))
    }

    /// Process cluster events
    pub async fn process_events(&mut self) -> P2pResult<()> {
        while let Some(event) = self.node.event_receiver().recv().await {
            match event {
                P2pEvent::PeerDiscovered(peer_id, info) => {
                    let role = match (info.is_control_plane, info.is_worker) {
                        (true, true) => MemberRole::Combined,
                        (true, false) => MemberRole::ControlPlane,
                        (false, true) => MemberRole::Worker,
                        (false, false) => MemberRole::Worker,
                    };

                    let member = ClusterMember {
                        info,
                        last_seen: chrono::Utc::now(),
                        role,
                    };

                    self.members.write().await.insert(peer_id, member);
                    info!("Added cluster member: {}", peer_id);

                    // Trigger leader election if needed
                    self.maybe_elect_leader().await;
                }

                P2pEvent::PeerDisconnected(peer_id) => {
                    self.members.write().await.remove(&peer_id);
                    info!("Removed cluster member: {}", peer_id);

                    // Re-elect leader if current leader disconnected
                    let current_leader = *self.leader.read().await;
                    if current_leader == Some(peer_id) {
                        self.elect_leader().await;
                    }
                }

                P2pEvent::MessageReceived(msg) => {
                    // Handle message based on type
                    match msg {
                        P2pMessage::ResourceUpdate(update) => {
                            // TODO: Apply update to local storage
                            info!(
                                "Resource update: {:?} {}/{:?}/{}",
                                update.action,
                                update.resource_type,
                                update.namespace,
                                update.name
                            );
                        }
                        P2pMessage::LeaderElection(election) => {
                            // Handle leader election
                            self.handle_leader_election(election).await;
                        }
                        _ => {}
                    }
                }

                P2pEvent::RequestReceived { peer, request } => {
                    // Handle direct requests
                    info!("Request from {}: {}", peer, request.request_type);
                }
            }
        }

        Ok(())
    }

    async fn maybe_elect_leader(&self) {
        let leader = self.leader.read().await;
        if leader.is_none() {
            drop(leader);
            self.elect_leader().await;
        }
    }

    async fn elect_leader(&self) {
        // Simple leader election: lowest peer ID among control plane nodes
        let members = self.members.read().await;

        let control_plane_peers: Vec<_> = members
            .iter()
            .filter(|(_, m)| {
                m.role == MemberRole::ControlPlane || m.role == MemberRole::Combined
            })
            .map(|(id, _)| *id)
            .collect();

        // Include self if we're control plane
        let mut candidates = control_plane_peers;
        // Note: In real implementation, we'd check our own role

        if candidates.is_empty() {
            return;
        }

        candidates.sort();
        let new_leader = candidates.first().copied();

        *self.leader.write().await = new_leader;

        if let Some(leader) = new_leader {
            info!("Elected new cluster leader: {}", leader);
        }
    }

    async fn handle_leader_election(&self, election: crate::message::LeaderElection) {
        // Simple leader election handling
        if election.vote_request {
            // Vote for the candidate if we haven't voted yet
            info!("Received vote request from {}", election.candidate_id);
        }
    }
}
