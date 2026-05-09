//! Container types shared across resources

use serde::{Deserialize, Serialize};

pub use super::common::ResourceRequirements;

/// Container definition
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    /// Name of the container
    pub name: String,

    /// Container image
    pub image: String,

    /// Image pull policy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_pull_policy: Option<ImagePullPolicy>,

    /// Command to run
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,

    /// Arguments to the command
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,

    /// Working directory
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Environment variables
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,

    /// Environment variables from sources
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_from: Vec<EnvFromSource>,

    /// Ports exposed by the container
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<ContainerPort>,

    /// Volume mounts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume_mounts: Vec<VolumeMount>,

    /// Resource requirements
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,

    /// Liveness probe
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liveness_probe: Option<Probe>,

    /// Readiness probe
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness_probe: Option<Probe>,

    /// Startup probe
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_probe: Option<Probe>,

    /// Security context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_context: Option<SecurityContext>,

    /// Stdin allocation
    #[serde(default)]
    pub stdin: bool,

    /// Stdin once
    #[serde(default)]
    pub stdin_once: bool,

    /// TTY allocation
    #[serde(default)]
    pub tty: bool,
}

/// Image pull policy
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum ImagePullPolicy {
    Always,
    Never,
    #[default]
    IfNotPresent,
}

/// Environment variable
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_from: Option<EnvVarSource>,
}

/// Source for an environment variable value
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVarSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map_key_ref: Option<ConfigMapKeySelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_key_ref: Option<SecretKeySelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_ref: Option<ObjectFieldSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_field_ref: Option<ResourceFieldSelector>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapKeySelector {
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretKeySelector {
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFieldSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    pub field_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceFieldSelector {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    pub resource: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divisor: Option<String>,
}

/// Environment from source
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvFromSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map_ref: Option<ConfigMapEnvSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_ref: Option<SecretEnvSource>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapEnvSource {
    pub name: String,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretEnvSource {
    pub name: String,
    #[serde(default)]
    pub optional: Option<bool>,
}

/// Container port
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerPort {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub container_port: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_port: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,
}

/// Network protocol
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Protocol {
    #[default]
    Tcp,
    Udp,
    Sctp,
}

/// Volume mount
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMount {
    pub name: String,
    pub mount_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_path_expr: Option<String>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mount_propagation: Option<MountPropagation>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum MountPropagation {
    #[default]
    None,
    HostToContainer,
    Bidirectional,
}

/// Health probe
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_get: Option<HttpGetAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_socket: Option<TcpSocketAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc: Option<GrpcAction>,

    #[serde(default)]
    pub initial_delay_seconds: i32,
    #[serde(default)]
    pub timeout_seconds: i32,
    #[serde(default)]
    pub period_seconds: i32,
    #[serde(default)]
    pub success_threshold: i32,
    #[serde(default)]
    pub failure_threshold: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpGetAction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub port: IntOrString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub http_headers: Vec<HttpHeader>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TcpSocketAction {
    pub port: IntOrString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ExecAction {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GrpcAction {
    pub port: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
}

/// Int or String value (used for ports)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum IntOrString {
    Int(i32),
    String(String),
}

impl Default for IntOrString {
    fn default() -> Self {
        IntOrString::Int(0)
    }
}

impl IntOrString {
    pub fn as_int(&self) -> Option<i32> {
        match self {
            IntOrString::Int(i) => Some(*i),
            IntOrString::String(s) => s.parse().ok(),
        }
    }
}

/// Security context for a container
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as_user: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as_group: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as_non_root: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only_root_filesystem: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privileged: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_privilege_escalation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Capabilities>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop: Vec<String>,
}

/// Container status
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub restart_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ContainerState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_state: Option<ContainerState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
    #[serde(default)]
    pub started: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<ContainerStateWaiting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<ContainerStateRunning>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminated: Option<ContainerStateTerminated>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateWaiting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateRunning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStateTerminated {
    pub exit_code: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}
