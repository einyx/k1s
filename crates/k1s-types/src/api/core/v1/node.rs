//! Node resource types

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Node is a worker node in the cluster
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<NodeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<NodeStatus>,
}

impl Resource for Node {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Node";
    const PLURAL: &'static str = "nodes";
    const SCOPE: ResourceScope = ResourceScope::Cluster;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl Node {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Node"),
            metadata: ObjectMeta::new(name),
            spec: Some(NodeSpec::default()),
            status: None,
        }
    }
}

/// NodeSpec describes the attributes of a node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSpec {
    /// PodCIDR is the pod IP range for this node
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_cidr: Option<String>,

    /// PodCIDRs is the list of IP ranges for this node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pod_cidrs: Vec<String>,

    /// Provider ID for cloud providers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,

    /// Unschedulable marks the node as unschedulable
    #[serde(default)]
    pub unschedulable: bool,

    /// Taints on the node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taints: Vec<Taint>,

    /// ConfigSource for dynamic kubelet config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_source: Option<NodeConfigSource>,
}

/// Taint on a node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Taint {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub effect: TaintEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_added: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum TaintEffect {
    #[default]
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

/// NodeConfigSource for dynamic kubelet config
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfigSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map: Option<ConfigMapNodeConfigSource>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapNodeConfigSource {
    pub namespace: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubelet_config_key: Option<String>,
}

/// NodeStatus contains information about the current status of a node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    /// Capacity represents the total resources of a node
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capacity: BTreeMap<String, String>,

    /// Allocatable represents the resources available for scheduling
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub allocatable: BTreeMap<String, String>,

    /// Conditions is an array of current node conditions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<NodeCondition>,

    /// Addresses is the list of addresses reachable to the node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<NodeAddress>,

    /// Daemon endpoints running on the node
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_endpoints: Option<NodeDaemonEndpoints>,

    /// Info about the node
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_info: Option<NodeSystemInfo>,

    /// Images on the node
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ContainerImage>,

    /// Volumes in use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes_in_use: Vec<String>,

    /// Volumes attached
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes_attached: Vec<AttachedVolume>,
}

/// NodeCondition contains information about the condition of a node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeCondition {
    pub r#type: NodeConditionType,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum NodeConditionType {
    #[default]
    Ready,
    MemoryPressure,
    DiskPressure,
    PIDPressure,
    NetworkUnavailable,
}

/// NodeAddress contains information for the node's address
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeAddress {
    pub r#type: NodeAddressType,
    pub address: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum NodeAddressType {
    Hostname,
    ExternalIP,
    #[default]
    InternalIP,
    ExternalDNS,
    InternalDNS,
}

/// NodeDaemonEndpoints lists ports opened by daemons running on the node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeDaemonEndpoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kubelet_endpoint: Option<DaemonEndpoint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonEndpoint {
    pub port: i32,
}

/// NodeSystemInfo contains detailed info about the node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSystemInfo {
    pub machine_id: String,
    pub system_uuid: String,
    pub boot_id: String,
    pub kernel_version: String,
    pub os_image: String,
    pub container_runtime_version: String,
    pub kubelet_version: String,
    pub kube_proxy_version: String,
    pub operating_system: String,
    pub architecture: String,
}

/// ContainerImage describes a container image
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerImage {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
}

/// AttachedVolume describes a volume attached to a node
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttachedVolume {
    pub name: String,
    pub device_path: String,
}
