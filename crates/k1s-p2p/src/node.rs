//! P2P node implementation

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use libp2p::{
    core::multiaddr::Protocol,
    gossipsub,
    identity::Keypair,
    kad,
    mdns,
    multiaddr::Multiaddr,
    request_response,
    swarm::{SwarmEvent, Swarm},
    PeerId,
};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::behaviour::{K1sBehaviour, K1sRequest, K1sResponse};
use crate::message::{topics, NodeInfo, P2pMessage};
use crate::{P2pError, P2pResult};

/// Configuration for a P2P node
#[derive(Debug, Clone)]
pub struct P2pConfig {
    pub node_name: String,
    pub listen_addr: SocketAddr,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub is_control_plane: bool,
    pub is_worker: bool,
    pub api_address: String,
    pub kubelet_port: u16,
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            node_name: "k1s-node".to_string(),
            listen_addr: "0.0.0.0:4001".parse().unwrap(),
            bootstrap_peers: vec![],
            is_control_plane: true,
            is_worker: true,
            api_address: "http://127.0.0.1:6443".to_string(),
            kubelet_port: 10250,
        }
    }
}

/// A P2P node in the k1s cluster
pub struct P2pNode {
    config: P2pConfig,
    swarm: Swarm<K1sBehaviour>,
    local_peer_id: PeerId,
    known_peers: Arc<RwLock<HashMap<PeerId, NodeInfo>>>,
    event_tx: mpsc::Sender<P2pEvent>,
    event_rx: mpsc::Receiver<P2pEvent>,
}

/// Events emitted by the P2P node
#[derive(Debug, Clone)]
pub enum P2pEvent {
    PeerDiscovered(PeerId, NodeInfo),
    PeerDisconnected(PeerId),
    MessageReceived(P2pMessage),
    RequestReceived { peer: PeerId, request: K1sRequest },
}

impl P2pNode {
    /// Create a new P2P node
    pub async fn new(config: P2pConfig) -> P2pResult<Self> {
        // Generate keypair
        let keypair = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(keypair.public());

        info!("Local peer ID: {}", local_peer_id);

        // Create behaviour
        let behaviour = K1sBehaviour::new(local_peer_id, &keypair);

        // Build swarm
        let swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_tcp(
                libp2p::tcp::Config::default(),
                libp2p::noise::Config::new,
                libp2p::yamux::Config::default,
            )
            .map_err(|e| P2pError::Transport(e.to_string()))?
            .with_behaviour(|_| behaviour)
            .map_err(|e| P2pError::Transport(e.to_string()))?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let (event_tx, event_rx) = mpsc::channel(1024);

        Ok(Self {
            config,
            swarm,
            local_peer_id,
            known_peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx,
        })
    }

    /// Get local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }

    /// Get known peers
    pub async fn known_peers(&self) -> HashMap<PeerId, NodeInfo> {
        self.known_peers.read().await.clone()
    }

    /// Start the P2P node
    pub async fn start(&mut self) -> P2pResult<()> {
        // Listen on configured address
        let listen_addr: Multiaddr = format!(
            "/ip4/{}/tcp/{}",
            self.config.listen_addr.ip(),
            self.config.listen_addr.port()
        )
        .parse()
        .map_err(|e| P2pError::Transport(format!("Invalid listen address: {}", e)))?;

        self.swarm
            .listen_on(listen_addr.clone())
            .map_err(|e| P2pError::Transport(e.to_string()))?;

        info!("Listening on {}", listen_addr);

        // Subscribe to topics
        self.swarm
            .behaviour_mut()
            .subscribe(topics::NODES)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;
        self.swarm
            .behaviour_mut()
            .subscribe(topics::PODS)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;
        self.swarm
            .behaviour_mut()
            .subscribe(topics::SERVICES)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;
        self.swarm
            .behaviour_mut()
            .subscribe(topics::EVENTS)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;

        // Connect to bootstrap peers
        for addr in &self.config.bootstrap_peers {
            info!("Connecting to bootstrap peer: {}", addr);
            if let Err(e) = self.swarm.dial(addr.clone()) {
                warn!("Failed to dial bootstrap peer {}: {}", addr, e);
            }
        }

        Ok(())
    }

    /// Run the event loop
    pub async fn run(&mut self) -> P2pResult<()> {
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await?;
                }
                _ = heartbeat_interval.tick() => {
                    self.send_heartbeat().await?;
                }
            }
        }
    }

    async fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<crate::behaviour::K1sBehaviourEvent>,
    ) -> P2pResult<()> {
        match event {
            SwarmEvent::Behaviour(crate::behaviour::K1sBehaviourEvent::Mdns(
                mdns::Event::Discovered(peers),
            )) => {
                for (peer_id, addr) in peers {
                    info!("mDNS discovered peer: {} at {}", peer_id, addr);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
            }

            SwarmEvent::Behaviour(crate::behaviour::K1sBehaviourEvent::Mdns(
                mdns::Event::Expired(peers),
            )) => {
                for (peer_id, _) in peers {
                    debug!("mDNS peer expired: {}", peer_id);
                }
            }

            SwarmEvent::Behaviour(crate::behaviour::K1sBehaviourEvent::Gossipsub(
                gossipsub::Event::Message {
                    propagation_source,
                    message_id,
                    message,
                },
            )) => {
                if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                    debug!("Received gossip message from {}: {:?}", propagation_source, msg);
                    self.handle_message(propagation_source, msg).await?;
                }
            }

            SwarmEvent::Behaviour(crate::behaviour::K1sBehaviourEvent::RequestResponse(
                request_response::Event::Message { peer, message },
            )) => {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        debug!("Received request from {}", peer);
                        let response = self.handle_request(peer, request).await;
                        if let Err(e) = self
                            .swarm
                            .behaviour_mut()
                            .request_response
                            .send_response(channel, response)
                        {
                            error!("Failed to send response: {:?}", e);
                        }
                    }
                    request_response::Message::Response { response, .. } => {
                        debug!("Received response from {}", peer);
                    }
                }
            }

            SwarmEvent::Behaviour(crate::behaviour::K1sBehaviourEvent::Kademlia(
                kad::Event::RoutingUpdated { peer, .. },
            )) => {
                debug!("Kademlia routing updated for peer: {}", peer);
            }

            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                info!("Connection established with {}", peer_id);
            }

            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                info!("Connection closed with {}: {:?}", peer_id, cause);
                self.known_peers.write().await.remove(&peer_id);
            }

            SwarmEvent::NewListenAddr { address, .. } => {
                info!("Listening on {}", address);
            }

            _ => {}
        }

        Ok(())
    }

    async fn handle_message(&mut self, peer: PeerId, msg: P2pMessage) -> P2pResult<()> {
        match msg {
            P2pMessage::NodeHeartbeat(info) => {
                info!("Received heartbeat from node: {}", info.node_name);
                self.known_peers.write().await.insert(peer, info.clone());
                let _ = self.event_tx.send(P2pEvent::PeerDiscovered(peer, info)).await;
            }
            P2pMessage::ResourceUpdate(update) => {
                debug!(
                    "Received resource update: {}/{:?}/{}",
                    update.resource_type, update.namespace, update.name
                );
                let _ = self
                    .event_tx
                    .send(P2pEvent::MessageReceived(P2pMessage::ResourceUpdate(update)))
                    .await;
            }
            _ => {
                let _ = self.event_tx.send(P2pEvent::MessageReceived(msg)).await;
            }
        }
        Ok(())
    }

    async fn handle_request(&mut self, peer: PeerId, request: K1sRequest) -> K1sResponse {
        // Handle different request types
        match request.request_type.as_str() {
            "get_node_info" => {
                let info = self.node_info();
                K1sResponse {
                    success: true,
                    payload: serde_json::to_vec(&info).unwrap_or_default(),
                    error: None,
                }
            }
            "sync_resources" => {
                // TODO: Return resources from storage
                K1sResponse {
                    success: true,
                    payload: vec![],
                    error: None,
                }
            }
            _ => K1sResponse {
                success: false,
                payload: vec![],
                error: Some(format!("Unknown request type: {}", request.request_type)),
            },
        }
    }

    async fn send_heartbeat(&mut self) -> P2pResult<()> {
        let info = self.node_info();
        let msg = P2pMessage::NodeHeartbeat(info);
        let data = serde_json::to_vec(&msg)?;

        self.swarm
            .behaviour_mut()
            .publish(topics::NODES, data)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;

        Ok(())
    }

    fn node_info(&self) -> NodeInfo {
        NodeInfo {
            node_name: self.config.node_name.clone(),
            peer_id: self.local_peer_id.to_string(),
            api_address: self.config.api_address.clone(),
            kubelet_port: self.config.kubelet_port,
            is_control_plane: self.config.is_control_plane,
            is_worker: self.config.is_worker,
            timestamp: chrono::Utc::now().timestamp(),
            capacity: crate::message::NodeCapacity::default(),
        }
    }

    /// Publish a message to a topic
    pub fn publish(&mut self, topic: &str, msg: &P2pMessage) -> P2pResult<()> {
        let data = serde_json::to_vec(msg)?;
        self.swarm
            .behaviour_mut()
            .publish(topic, data)
            .map_err(|e| P2pError::Protocol(e.to_string()))?;
        Ok(())
    }

    /// Send a request to a specific peer
    pub fn send_request(&mut self, peer: PeerId, request: K1sRequest) -> request_response::OutboundRequestId {
        self.swarm
            .behaviour_mut()
            .request_response
            .send_request(&peer, request)
    }

    /// Get the event receiver
    pub fn event_receiver(&mut self) -> &mut mpsc::Receiver<P2pEvent> {
        &mut self.event_rx
    }
}
