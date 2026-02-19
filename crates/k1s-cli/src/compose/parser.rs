//! Docker Compose file parser

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Docker Compose file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFile {
    #[serde(default)]
    pub version: Option<String>,

    pub services: BTreeMap<String, ComposeService>,

    #[serde(default)]
    pub networks: BTreeMap<String, ComposeNetwork>,

    #[serde(default)]
    pub volumes: BTreeMap<String, ComposeVolume>,

    #[serde(default)]
    pub configs: BTreeMap<String, ComposeConfig>,

    #[serde(default)]
    pub secrets: BTreeMap<String, ComposeSecret>,
}

/// Compose service definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeService {
    #[serde(default)]
    pub image: Option<String>,

    #[serde(default)]
    pub build: Option<ComposeBuild>,

    #[serde(default)]
    pub command: Option<ComposeCommand>,

    #[serde(default)]
    pub entrypoint: Option<ComposeCommand>,

    #[serde(default)]
    pub environment: Option<ComposeEnvironment>,

    #[serde(default)]
    pub env_file: Option<ComposeStringOrList>,

    #[serde(default)]
    pub ports: Vec<ComposePort>,

    #[serde(default)]
    pub expose: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<ComposeVolumeMount>,

    #[serde(default)]
    pub networks: Option<ComposeServiceNetworks>,

    #[serde(default)]
    pub depends_on: Option<ComposeDependsOn>,

    #[serde(default)]
    pub deploy: Option<ComposeDeploy>,

    #[serde(default)]
    pub restart: Option<String>,

    #[serde(default)]
    pub healthcheck: Option<ComposeHealthcheck>,

    #[serde(default)]
    pub labels: Option<ComposeLabels>,

    #[serde(default)]
    pub working_dir: Option<String>,

    #[serde(default)]
    pub user: Option<String>,

    #[serde(default)]
    pub hostname: Option<String>,

    #[serde(default)]
    pub container_name: Option<String>,

    #[serde(default)]
    pub privileged: Option<bool>,

    #[serde(default)]
    pub cap_add: Vec<String>,

    #[serde(default)]
    pub cap_drop: Vec<String>,

    #[serde(default)]
    pub configs: Vec<ComposeConfigRef>,

    #[serde(default)]
    pub secrets: Vec<ComposeSecretRef>,

    #[serde(default)]
    pub logging: Option<ComposeLogging>,

    #[serde(default)]
    pub extra_hosts: Vec<String>,

    #[serde(default)]
    pub ulimits: Option<ComposeUlimits>,
}

/// Build configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeBuild {
    Simple(String),
    Extended {
        context: String,
        #[serde(default)]
        dockerfile: Option<String>,
        #[serde(default)]
        args: Option<ComposeEnvironment>,
        #[serde(default)]
        target: Option<String>,
    },
}

/// Command can be string or list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeCommand {
    Simple(String),
    List(Vec<String>),
}

impl ComposeCommand {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            ComposeCommand::Simple(s) => {
                // Parse shell command
                shell_words::split(s).unwrap_or_else(|_| vec![s.clone()])
            }
            ComposeCommand::List(l) => l.clone(),
        }
    }
}

/// Environment variables
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeEnvironment {
    List(Vec<String>),
    Map(BTreeMap<String, Option<String>>),
}

impl ComposeEnvironment {
    pub fn to_map(&self) -> BTreeMap<String, String> {
        match self {
            ComposeEnvironment::List(list) => {
                list.iter()
                    .filter_map(|s| {
                        let parts: Vec<&str> = s.splitn(2, '=').collect();
                        if parts.len() == 2 {
                            Some((parts[0].to_string(), parts[1].to_string()))
                        } else {
                            // Variable reference (get from env)
                            std::env::var(parts[0])
                                .ok()
                                .map(|v| (parts[0].to_string(), v))
                        }
                    })
                    .collect()
            }
            ComposeEnvironment::Map(map) => {
                map.iter()
                    .filter_map(|(k, v)| {
                        v.clone()
                            .or_else(|| std::env::var(k).ok())
                            .map(|val| (k.clone(), val))
                    })
                    .collect()
            }
        }
    }
}

/// String or list of strings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeStringOrList {
    Single(String),
    List(Vec<String>),
}

/// Port mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposePort {
    Simple(String),
    Extended {
        target: u16,
        #[serde(default)]
        published: Option<u16>,
        #[serde(default)]
        protocol: Option<String>,
        #[serde(default)]
        mode: Option<String>,
    },
}

impl ComposePort {
    pub fn parse(&self) -> (Option<u16>, u16, String) {
        match self {
            ComposePort::Simple(s) => {
                // Parse formats: "80", "80:80", "127.0.0.1:80:80", "80:80/udp"
                let (port_str, protocol) = if let Some(pos) = s.rfind('/') {
                    (&s[..pos], s[pos + 1..].to_string())
                } else {
                    (s.as_str(), "tcp".to_string())
                };

                let parts: Vec<&str> = port_str.split(':').collect();
                match parts.len() {
                    1 => {
                        let port = parts[0].parse().unwrap_or(80);
                        (None, port, protocol)
                    }
                    2 => {
                        let host = parts[0].parse().ok();
                        let container = parts[1].parse().unwrap_or(80);
                        (host, container, protocol)
                    }
                    3 => {
                        // ip:host:container
                        let host = parts[1].parse().ok();
                        let container = parts[2].parse().unwrap_or(80);
                        (host, container, protocol)
                    }
                    _ => (None, 80, protocol),
                }
            }
            ComposePort::Extended {
                target,
                published,
                protocol,
                ..
            } => (
                *published,
                *target,
                protocol.clone().unwrap_or_else(|| "tcp".to_string()),
            ),
        }
    }
}

/// Volume mount
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeVolumeMount {
    Simple(String),
    Extended {
        #[serde(rename = "type")]
        mount_type: String,
        source: Option<String>,
        target: String,
        #[serde(default)]
        read_only: bool,
        #[serde(default)]
        bind: Option<ComposeBind>,
        #[serde(default)]
        volume: Option<ComposeVolumeOptions>,
    },
}

impl ComposeVolumeMount {
    pub fn parse(&self) -> (Option<String>, String, bool) {
        match self {
            ComposeVolumeMount::Simple(s) => {
                let parts: Vec<&str> = s.split(':').collect();
                match parts.len() {
                    1 => (None, parts[0].to_string(), false),
                    2 => {
                        let read_only = parts[1] == "ro";
                        if read_only {
                            (None, parts[0].to_string(), true)
                        } else {
                            (Some(parts[0].to_string()), parts[1].to_string(), false)
                        }
                    }
                    3 => {
                        let read_only = parts[2] == "ro";
                        (Some(parts[0].to_string()), parts[1].to_string(), read_only)
                    }
                    _ => (None, s.clone(), false),
                }
            }
            ComposeVolumeMount::Extended {
                source,
                target,
                read_only,
                ..
            } => (source.clone(), target.clone(), *read_only),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeBind {
    #[serde(default)]
    pub propagation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeVolumeOptions {
    #[serde(default)]
    pub nocopy: bool,
}

/// Service networks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeServiceNetworks {
    List(Vec<String>),
    Map(BTreeMap<String, Option<ComposeNetworkConfig>>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeNetworkConfig {
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub ipv4_address: Option<String>,
}

/// depends_on
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeDependsOn {
    List(Vec<String>),
    Map(BTreeMap<String, ComposeDependency>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeDependency {
    pub condition: String,
}

/// Placement constraints and preferences (Swarm)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposePlacement {
    /// Constraint expressions like "node.role == manager"
    #[serde(default)]
    pub constraints: Vec<String>,
}

/// Rolling update / rollout configuration (Swarm)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeUpdateConfig {
    #[serde(default)]
    pub parallelism: Option<u32>,
    #[serde(default)]
    pub delay: Option<String>,
    /// "start-first" | "stop-first"
    #[serde(default)]
    pub order: Option<String>,
    /// "pause" | "continue" | "rollback"
    #[serde(default)]
    pub failure_action: Option<String>,
}

/// Deploy configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeDeploy {
    #[serde(default)]
    pub replicas: Option<u32>,

    /// "replicated" (default) or "global" (maps to DaemonSet)
    #[serde(default)]
    pub mode: Option<String>,

    #[serde(default)]
    pub placement: Option<ComposePlacement>,

    #[serde(default)]
    pub update_config: Option<ComposeUpdateConfig>,

    #[serde(default)]
    pub rollback_config: Option<ComposeUpdateConfig>,

    /// "vip" (default ClusterIP) or "dnsrr" (headless service)
    #[serde(default)]
    pub endpoint_mode: Option<String>,

    #[serde(default)]
    pub resources: Option<ComposeResources>,

    #[serde(default)]
    pub restart_policy: Option<ComposeRestartPolicy>,

    #[serde(default)]
    pub labels: Option<ComposeLabels>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeResources {
    #[serde(default)]
    pub limits: Option<ComposeResourceSpec>,
    #[serde(default)]
    pub reservations: Option<ComposeResourceSpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeResourceSpec {
    #[serde(default)]
    pub cpus: Option<String>,
    #[serde(default)]
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeRestartPolicy {
    pub condition: String,
    #[serde(default)]
    pub delay: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub window: Option<String>,
}

/// Healthcheck
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeHealthcheck {
    #[serde(default)]
    pub test: Option<ComposeCommand>,
    #[serde(default)]
    pub interval: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub retries: Option<u32>,
    #[serde(default)]
    pub start_period: Option<String>,
    #[serde(default)]
    pub disable: bool,
}

/// Labels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeLabels {
    List(Vec<String>),
    Map(BTreeMap<String, String>),
}

impl ComposeLabels {
    pub fn to_map(&self) -> BTreeMap<String, String> {
        match self {
            ComposeLabels::List(list) => {
                list.iter()
                    .filter_map(|s| {
                        let parts: Vec<&str> = s.splitn(2, '=').collect();
                        if parts.len() == 2 {
                            Some((parts[0].to_string(), parts[1].to_string()))
                        } else {
                            None
                        }
                    })
                    .collect()
            }
            ComposeLabels::Map(map) => map.clone(),
        }
    }
}

/// Config reference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeConfigRef {
    Simple(String),
    Extended {
        source: String,
        target: String,
        #[serde(default)]
        uid: Option<String>,
        #[serde(default)]
        gid: Option<String>,
        #[serde(default)]
        mode: Option<u32>,
    },
}

/// Secret reference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeSecretRef {
    Simple(String),
    Extended {
        source: String,
        target: String,
        #[serde(default)]
        uid: Option<String>,
        #[serde(default)]
        gid: Option<String>,
        #[serde(default)]
        mode: Option<u32>,
    },
}

/// Network definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeNetwork {
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub external: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Volume definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeVolume {
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub external: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Config definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeConfig {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub external: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Secret definition
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeSecret {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub external: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Logging configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeLogging {
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub options: Option<BTreeMap<String, String>>,
}

/// Ulimits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeUlimits {
    Map(BTreeMap<String, ComposeUlimit>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComposeUlimit {
    Single(i64),
    Extended { soft: i64, hard: i64 },
}

impl ComposeFile {
    /// Parse from YAML string
    pub fn parse(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Parse from file
    pub fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::parse(&content)?)
    }
}
