//! libp2p behaviour composition

use libp2p::{
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identify,
    kad::{self, store::MemoryStore},
    mdns,
    request_response::{self, ProtocolSupport},
    swarm::NetworkBehaviour,
    PeerId,
};
use std::time::Duration;

/// Protocol name for request-response
pub const K1S_PROTOCOL: &str = "/k1s/1.0.0";

/// Combined network behaviour for k1s
#[derive(NetworkBehaviour)]
pub struct K1sBehaviour {
    /// Kademlia DHT for peer discovery and routing
    pub kademlia: kad::Behaviour<MemoryStore>,

    /// mDNS for local network discovery
    pub mdns: mdns::tokio::Behaviour,

    /// GossipSub for pub/sub messaging
    pub gossipsub: gossipsub::Behaviour,

    /// Identify protocol for peer info exchange
    pub identify: identify::Behaviour,

    /// Request-response for direct communication
    pub request_response: request_response::cbor::Behaviour<K1sRequest, K1sResponse>,
}

/// Request type for request-response protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct K1sRequest {
    pub request_type: String,
    pub payload: Vec<u8>,
}

/// Response type for request-response protocol
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct K1sResponse {
    pub success: bool,
    pub payload: Vec<u8>,
    pub error: Option<String>,
}

impl K1sBehaviour {
    pub fn new(local_peer_id: PeerId, keypair: &libp2p::identity::Keypair) -> Self {
        // Kademlia
        let store = MemoryStore::new(local_peer_id);
        let kademlia = kad::Behaviour::new(local_peer_id, store);

        // mDNS
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)
            .expect("Failed to create mDNS behaviour");

        // GossipSub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .build()
            .expect("Valid gossipsub config");

        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(keypair.clone()),
            gossipsub_config,
        )
        .expect("Failed to create gossipsub behaviour");

        // Identify
        let identify = identify::Behaviour::new(identify::Config::new(
            K1S_PROTOCOL.to_string(),
            keypair.public(),
        ));

        // Request-Response
        let request_response = request_response::cbor::Behaviour::new(
            [(
                libp2p::StreamProtocol::new(K1S_PROTOCOL),
                ProtocolSupport::Full,
            )],
            request_response::Config::default(),
        );

        Self {
            kademlia,
            mdns,
            gossipsub,
            identify,
            request_response,
        }
    }

    /// Subscribe to a topic
    pub fn subscribe(&mut self, topic: &str) -> Result<bool, gossipsub::SubscriptionError> {
        let topic = IdentTopic::new(topic);
        self.gossipsub.subscribe(&topic)
    }

    /// Publish to a topic
    pub fn publish(
        &mut self,
        topic: &str,
        data: Vec<u8>,
    ) -> Result<gossipsub::MessageId, gossipsub::PublishError> {
        let topic = IdentTopic::new(topic);
        self.gossipsub.publish(topic, data)
    }
}
