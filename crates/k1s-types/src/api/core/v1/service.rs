//! Service resource types

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::container::Protocol;

/// Service is a named abstraction of software service
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<ServiceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ServiceStatus>,
}

impl Resource for Service {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Service";
    const PLURAL: &'static str = "services";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl Service {
    pub fn new(name: &str, spec: ServiceSpec) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Service"),
            metadata: ObjectMeta::new(name),
            spec: Some(spec),
            status: None,
        }
    }

    pub fn namespaced(name: &str, namespace: &str, spec: ServiceSpec) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Service"),
            metadata: ObjectMeta::namespaced(name, namespace),
            spec: Some(spec),
            status: None,
        }
    }
}

/// ServiceSpec describes the attributes of a service
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSpec {
    /// Ports exposed by the service
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<ServicePort>,

    /// Label selector for pods
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub selector: BTreeMap<String, String>,

    /// ClusterIP is the IP address of the service
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_ip: Option<String>,

    /// ClusterIPs is a list of IPs assigned to the service
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cluster_ips: Vec<String>,

    /// Type of the service
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ServiceType>,

    /// External IPs for the service
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_ips: Vec<String>,

    /// Session affinity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_affinity: Option<SessionAffinity>,

    /// Session affinity config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_affinity_config: Option<SessionAffinityConfig>,

    /// LoadBalancer IP (deprecated, use loadBalancerClass)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_ip: Option<String>,

    /// LoadBalancer source ranges
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_balancer_source_ranges: Vec<String>,

    /// LoadBalancer class
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer_class: Option<String>,

    /// External name (for ExternalName type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_name: Option<String>,

    /// External traffic policy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_traffic_policy: Option<ServiceExternalTrafficPolicy>,

    /// Internal traffic policy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_traffic_policy: Option<ServiceInternalTrafficPolicy>,

    /// Health check node port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check_node_port: Option<i32>,

    /// Publish not ready addresses
    #[serde(default)]
    pub publish_not_ready_addresses: bool,

    /// IP families
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ip_families: Vec<IPFamily>,

    /// IP family policy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip_family_policy: Option<IPFamilyPolicy>,

    /// Allocate load balancer node ports
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocate_load_balancer_node_ports: Option<bool>,
}

/// Service port
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServicePort {
    /// Name of the port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Protocol (TCP, UDP, SCTP)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,

    /// App protocol
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_protocol: Option<String>,

    /// Port that the service exposes
    pub port: i32,

    /// Target port on the pod
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_port: Option<super::container::IntOrString>,

    /// Node port (for NodePort/LoadBalancer types)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_port: Option<i32>,
}

/// Service type
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum ServiceType {
    #[default]
    ClusterIP,
    NodePort,
    LoadBalancer,
    ExternalName,
}

/// Session affinity
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum SessionAffinity {
    #[default]
    None,
    ClientIP,
}

/// Session affinity config
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionAffinityConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<ClientIPConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientIPConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,
}

/// External traffic policy
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum ServiceExternalTrafficPolicy {
    #[default]
    Cluster,
    Local,
}

/// Internal traffic policy
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum ServiceInternalTrafficPolicy {
    #[default]
    Cluster,
    Local,
}

/// IP family
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum IPFamily {
    #[default]
    IPv4,
    IPv6,
}

/// IP family policy
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum IPFamilyPolicy {
    #[default]
    SingleStack,
    PreferDualStack,
    RequireDualStack,
}

/// ServiceStatus describes the current status of a service
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    /// LoadBalancer status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_balancer: Option<LoadBalancerStatus>,

    /// Conditions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ServiceCondition>,
}

/// LoadBalancer status
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancerStatus {
    /// Ingress points for the load balancer
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ingress: Vec<LoadBalancerIngress>,
}

/// LoadBalancer ingress point
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancerIngress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<PortStatus>,
}

/// Port status
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortStatus {
    pub port: i32,
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Service condition
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
