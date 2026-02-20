//! Endpoints resource types
//!
//! Endpoints are a collection of network endpoints that implement a Service.

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};

use super::common::ObjectReference;
use super::container::Protocol;

/// Endpoints is a collection of endpoints that implement the actual service
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Endpoints {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// The set of all endpoints is the union of all subsets
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subsets: Vec<EndpointSubset>,
}

impl Resource for Endpoints {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Endpoints";
    const PLURAL: &'static str = "endpoints";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl Endpoints {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Endpoints"),
            metadata: ObjectMeta::new(name),
            subsets: Vec::new(),
        }
    }

    pub fn namespaced(name: &str, namespace: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Endpoints"),
            metadata: ObjectMeta::namespaced(name, namespace),
            subsets: Vec::new(),
        }
    }

    /// Create endpoints with subsets
    pub fn with_subsets(mut self, subsets: Vec<EndpointSubset>) -> Self {
        self.subsets = subsets;
        self
    }
}

/// EndpointSubset is a group of addresses with a common set of ports
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointSubset {
    /// IP addresses which offer the related ports that are marked as ready
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<EndpointAddress>,
    /// IP addresses which offer the related ports but are not currently marked ready
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_ready_addresses: Vec<EndpointAddress>,
    /// Port numbers available on the related IP addresses
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<EndpointPort>,
}

impl EndpointSubset {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_addresses(mut self, addresses: Vec<EndpointAddress>) -> Self {
        self.addresses = addresses;
        self
    }

    pub fn with_not_ready_addresses(mut self, addresses: Vec<EndpointAddress>) -> Self {
        self.not_ready_addresses = addresses;
        self
    }

    pub fn with_ports(mut self, ports: Vec<EndpointPort>) -> Self {
        self.ports = ports;
        self
    }
}

/// EndpointAddress is a tuple that describes single IP address
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointAddress {
    /// The IP of this endpoint
    pub ip: String,
    /// The Hostname of this endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// Node hosting this endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// Reference to object providing the endpoint
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<ObjectReference>,
}

impl EndpointAddress {
    pub fn new(ip: &str) -> Self {
        Self {
            ip: ip.to_string(),
            ..Default::default()
        }
    }

    pub fn with_node_name(mut self, node_name: &str) -> Self {
        self.node_name = Some(node_name.to_string());
        self
    }

    pub fn with_target_ref(mut self, target_ref: ObjectReference) -> Self {
        self.target_ref = Some(target_ref);
        self
    }
}

/// EndpointPort is a tuple that describes a single port
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointPort {
    /// The name of this port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The port number of the endpoint
    pub port: i32,
    /// The IP protocol for this port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,
    /// The application protocol for this port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_protocol: Option<String>,
}

impl EndpointPort {
    pub fn new(port: i32) -> Self {
        Self {
            port,
            ..Default::default()
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_protocol(mut self, protocol: Protocol) -> Self {
        self.protocol = Some(protocol);
        self
    }
}
