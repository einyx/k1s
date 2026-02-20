//! Translate Docker Compose / Docker Swarm stack files to Kubernetes resources

use std::collections::BTreeMap;

use k1s_types::{
    apps_v1::{DaemonSet, DaemonSetSpec, Deployment, DeploymentSpec, DeploymentStrategy,
              DeploymentStrategyType, LabelSelector, PodTemplateSpec, RollingUpdateDeployment},
    ConfigMap, Container, ContainerPort, EnvVar, ObjectMeta, Pod, PodSpec, Protocol,
    ResourceRequirements, Secret, Service, ServicePort, ServiceSpec, ServiceType, TypeMeta,
    Volume, VolumeMount, VolumeSource, ConfigMapVolumeSource, SecretVolumeSource,
    EmptyDirVolumeSource, HostPathVolumeSource,
};

use super::parser::{ComposeFile, ComposePort, ComposeService};

/// Kubernetes resources generated from a Compose / Swarm stack file
#[derive(Debug, Default)]
pub struct K8sResources {
    pub config_maps: Vec<ConfigMap>,
    pub secrets: Vec<Secret>,
    pub services: Vec<Service>,
    /// Bare Pods (compose files without a deploy section)
    pub pods: Vec<Pod>,
    /// Deployments (replicated Swarm services or compose services with replicas)
    pub deployments: Vec<Deployment>,
    /// DaemonSets (Swarm global-mode services)
    pub daemon_sets: Vec<DaemonSet>,
}

impl K8sResources {
    /// Convert all resources to YAML
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        let mut docs = Vec::new();

        for cm in &self.config_maps {
            docs.push(serde_yaml::to_string(cm)?);
        }
        for secret in &self.secrets {
            docs.push(serde_yaml::to_string(secret)?);
        }
        for svc in &self.services {
            docs.push(serde_yaml::to_string(svc)?);
        }
        for pod in &self.pods {
            docs.push(serde_yaml::to_string(pod)?);
        }
        for deploy in &self.deployments {
            docs.push(serde_yaml::to_string(deploy)?);
        }
        for ds in &self.daemon_sets {
            docs.push(serde_yaml::to_string(ds)?);
        }

        Ok(docs.join("---\n"))
    }
}

/// Compose to Kubernetes translator
pub struct ComposeTranslator {
    namespace: String,
    app_name: String,
}

impl ComposeTranslator {
    pub fn new(namespace: &str, app_name: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            app_name: app_name.to_string(),
        }
    }

    /// Translate a Compose file to Kubernetes resources
    pub fn translate(&self, compose: &ComposeFile) -> K8sResources {
        let mut resources = K8sResources::default();

        // Translate configs to ConfigMaps
        for (name, config) in &compose.configs {
            if let Some(file) = &config.file {
                if let Ok(content) = std::fs::read_to_string(file) {
                    let mut data = BTreeMap::new();
                    // Use filename as key
                    let key = std::path::Path::new(file)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| name.clone());
                    data.insert(key, content);

                    let cm = ConfigMap {
                        type_meta: TypeMeta::new("v1", "ConfigMap"),
                        metadata: ObjectMeta::namespaced(name, &self.namespace),
                        data,
                        ..Default::default()
                    };
                    resources.config_maps.push(cm);
                }
            }
        }

        // Translate secrets
        for (name, secret) in &compose.secrets {
            if let Some(file) = &secret.file {
                if let Ok(content) = std::fs::read_to_string(file) {
                    use base64::{engine::general_purpose::STANDARD, Engine};
                    let mut data = BTreeMap::new();
                    let key = std::path::Path::new(file)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| name.clone());
                    data.insert(key, STANDARD.encode(content.as_bytes()));

                    let secret = Secret {
                        type_meta: TypeMeta::new("v1", "Secret"),
                        metadata: ObjectMeta::namespaced(name, &self.namespace),
                        data,
                        ..Default::default()
                    };
                    resources.secrets.push(secret);
                }
            }
        }

        // Translate services
        for (name, service) in &compose.services {
            let deploy = service.deploy.as_ref();
            let mode = deploy.and_then(|d| d.mode.as_deref()).unwrap_or("replicated");
            let has_replicas = deploy.and_then(|d| d.replicas).is_some();
            let has_deploy = service.deploy.is_some();

            if mode == "global" {
                // Swarm global mode → DaemonSet (one pod per node)
                let ds = self.translate_to_daemon_set(name, service);
                resources.daemon_sets.push(ds);
            } else if has_deploy || has_replicas {
                // Swarm replicated service or compose with deploy section → Deployment
                let deployment = self.translate_to_deployment(name, service);
                resources.deployments.push(deployment);
            } else {
                // Plain compose service with no deploy section → bare Pod
                let pod = self.translate_service(name, service);
                resources.pods.push(pod);
            }

            // Create Service if ports are exposed
            if !service.ports.is_empty() {
                let k8s_service = self.translate_service_network(name, service);
                resources.services.push(k8s_service);
            }
        }

        resources
    }

    fn translate_to_deployment(&self, name: &str, service: &ComposeService) -> Deployment {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), name.to_string());
        labels.insert("compose.app".to_string(), self.app_name.clone());

        if let Some(compose_labels) = &service.labels {
            for (k, v) in compose_labels.to_map() {
                labels.insert(k, v);
            }
        }

        let deploy = service.deploy.as_ref();
        let replicas = deploy.and_then(|d| d.replicas).map(|r| r as i32).unwrap_or(1);

        // Translate placement constraints → nodeSelector
        let node_selector = deploy
            .and_then(|d| d.placement.as_ref())
            .map(|p| parse_placement_constraints(&p.constraints))
            .unwrap_or_default();

        // Build pod template spec
        let container = self.translate_container(name, service);
        let volumes = self.translate_volumes(name, service);
        let restart_policy = service.restart.as_ref().map(|r| match r.as_str() {
            "no" => k1s_types::RestartPolicy::Never,
            "on-failure" => k1s_types::RestartPolicy::OnFailure,
            _ => k1s_types::RestartPolicy::Always,
        });

        let pod_spec = PodSpec {
            containers: vec![container],
            volumes,
            hostname: service.hostname.clone(),
            restart_policy,
            node_selector,
            ..Default::default()
        };

        let mut template_meta = ObjectMeta::namespaced(name, &self.namespace);
        template_meta.labels = labels.clone();

        // Rolling update strategy from update_config
        let strategy = deploy.and_then(|d| d.update_config.as_ref()).map(|uc| {
            DeploymentStrategy {
                r#type: Some(DeploymentStrategyType::RollingUpdate),
                rolling_update: Some(RollingUpdateDeployment {
                    max_unavailable: uc.parallelism.map(|p| p as i32),
                    max_surge: None,
                }),
            }
        });

        Deployment {
            type_meta: TypeMeta::new("apps/v1", "Deployment"),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(self.namespace.clone()),
                labels: labels.clone(),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                replicas: Some(replicas),
                selector: LabelSelector {
                    match_labels: {
                        let mut sel = BTreeMap::new();
                        sel.insert("app".to_string(), name.to_string());
                        sel
                    },
                    match_expressions: Vec::new(),
                },
                template: PodTemplateSpec {
                    metadata: template_meta,
                    spec: Some(pod_spec),
                },
                strategy,
                ..Default::default()
            }),
            status: None,
        }
    }

    fn translate_to_daemon_set(&self, name: &str, service: &ComposeService) -> DaemonSet {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), name.to_string());
        labels.insert("compose.app".to_string(), self.app_name.clone());

        if let Some(compose_labels) = &service.labels {
            for (k, v) in compose_labels.to_map() {
                labels.insert(k, v);
            }
        }

        let deploy = service.deploy.as_ref();
        let node_selector = deploy
            .and_then(|d| d.placement.as_ref())
            .map(|p| parse_placement_constraints(&p.constraints))
            .unwrap_or_default();

        let container = self.translate_container(name, service);
        let volumes = self.translate_volumes(name, service);
        let restart_policy = service.restart.as_ref().map(|r| match r.as_str() {
            "no" => k1s_types::RestartPolicy::Never,
            "on-failure" => k1s_types::RestartPolicy::OnFailure,
            _ => k1s_types::RestartPolicy::Always,
        });

        let pod_spec = PodSpec {
            containers: vec![container],
            volumes,
            hostname: service.hostname.clone(),
            restart_policy,
            node_selector,
            ..Default::default()
        };

        let mut template_meta = ObjectMeta::namespaced(name, &self.namespace);
        template_meta.labels = labels.clone();

        DaemonSet {
            type_meta: TypeMeta::new("apps/v1", "DaemonSet"),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(self.namespace.clone()),
                labels: labels.clone(),
                ..Default::default()
            },
            spec: Some(DaemonSetSpec {
                selector: LabelSelector {
                    match_labels: {
                        let mut sel = BTreeMap::new();
                        sel.insert("app".to_string(), name.to_string());
                        sel
                    },
                    match_expressions: Vec::new(),
                },
                template: PodTemplateSpec {
                    metadata: template_meta,
                    spec: Some(pod_spec),
                },
                min_ready_seconds: None,
                revision_history_limit: None,
                update_strategy: None,
            }),
            status: None,
        }
    }

    fn translate_service(&self, name: &str, service: &ComposeService) -> Pod {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), name.to_string());
        labels.insert("compose.app".to_string(), self.app_name.clone());

        // Add custom labels
        if let Some(compose_labels) = &service.labels {
            for (k, v) in compose_labels.to_map() {
                labels.insert(k, v);
            }
        }

        // Build container
        let container = self.translate_container(name, service);

        // Build volumes
        let volumes = self.translate_volumes(name, service);

        let spec = PodSpec {
            containers: vec![container],
            volumes,
            hostname: service.hostname.clone(),
            restart_policy: service.restart.as_ref().map(|r| match r.as_str() {
                "no" => k1s_types::RestartPolicy::Never,
                "on-failure" => k1s_types::RestartPolicy::OnFailure,
                _ => k1s_types::RestartPolicy::Always,
            }),
            ..Default::default()
        };

        Pod {
            type_meta: TypeMeta::new("v1", "Pod"),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(self.namespace.clone()),
                labels,
                ..Default::default()
            },
            spec: Some(spec),
            status: None,
        }
    }

    fn translate_container(&self, name: &str, service: &ComposeService) -> Container {
        let image = service.image.clone().unwrap_or_else(|| {
            // If no image, use build context as hint
            format!("{}:latest", name)
        });

        // Environment variables
        let env = service
            .environment
            .as_ref()
            .map(|e| {
                e.to_map()
                    .into_iter()
                    .map(|(k, v)| EnvVar {
                        name: k,
                        value: Some(v),
                        value_from: None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Ports
        let ports = service
            .ports
            .iter()
            .map(|p| {
                let (_, container_port, protocol) = p.parse();
                ContainerPort {
                    container_port: container_port as i32,
                    protocol: Some(match protocol.to_lowercase().as_str() {
                        "udp" => Protocol::Udp,
                        "sctp" => Protocol::Sctp,
                        _ => Protocol::Tcp,
                    }),
                    ..Default::default()
                }
            })
            .collect();

        // Volume mounts
        let volume_mounts = service
            .volumes
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let (_, target, read_only) = v.parse();
                VolumeMount {
                    name: format!("{}-vol-{}", name, i),
                    mount_path: target,
                    read_only,
                    ..Default::default()
                }
            })
            .collect();

        // Command and args
        let (command, args) = match (&service.entrypoint, &service.command) {
            (Some(ep), Some(cmd)) => (ep.to_vec(), cmd.to_vec()),
            (Some(ep), None) => (ep.to_vec(), vec![]),
            (None, Some(cmd)) => (vec![], cmd.to_vec()),
            (None, None) => (vec![], vec![]),
        };

        // Resources
        let resources = service.deploy.as_ref().and_then(|d| {
            d.resources.as_ref().map(|r| {
                let mut limits = BTreeMap::new();
                let mut requests = BTreeMap::new();

                if let Some(lim) = &r.limits {
                    if let Some(cpu) = &lim.cpus {
                        limits.insert("cpu".to_string(), cpu.clone());
                    }
                    if let Some(mem) = &lim.memory {
                        limits.insert("memory".to_string(), mem.clone());
                    }
                }

                if let Some(res) = &r.reservations {
                    if let Some(cpu) = &res.cpus {
                        requests.insert("cpu".to_string(), cpu.clone());
                    }
                    if let Some(mem) = &res.memory {
                        requests.insert("memory".to_string(), mem.clone());
                    }
                }

                ResourceRequirements { limits, requests }
            })
        });

        // Security context
        let security_context = if service.privileged.unwrap_or(false)
            || !service.cap_add.is_empty()
            || !service.cap_drop.is_empty()
        {
            Some(k1s_types::SecurityContext {
                privileged: service.privileged,
                capabilities: if service.cap_add.is_empty() && service.cap_drop.is_empty() {
                    None
                } else {
                    Some(k1s_types::Capabilities {
                        add: service.cap_add.clone(),
                        drop: service.cap_drop.clone(),
                    })
                },
                ..Default::default()
            })
        } else {
            None
        };

        Container {
            name: name.to_string(),
            image,
            command: if command.is_empty() { vec![] } else { command },
            args: if args.is_empty() { vec![] } else { args },
            env,
            ports,
            volume_mounts,
            resources,
            security_context,
            working_dir: service.working_dir.clone(),
            ..Default::default()
        }
    }

    fn translate_volumes(&self, name: &str, service: &ComposeService) -> Vec<Volume> {
        service
            .volumes
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let (source, _, _) = v.parse();
                let vol_name = format!("{}-vol-{}", name, i);

                let source = match source {
                    Some(src) if src.starts_with('/') || src.starts_with('.') => {
                        // Host path
                        VolumeSource {
                            host_path: Some(HostPathVolumeSource {
                                path: src,
                                ..Default::default()
                            }),
                            ..Default::default()
                        }
                    }
                    Some(src) => {
                        // Named volume - use emptyDir for now
                        // In real implementation, would map to PVC
                        VolumeSource {
                            empty_dir: Some(EmptyDirVolumeSource::default()),
                            ..Default::default()
                        }
                    }
                    None => {
                        // Anonymous volume
                        VolumeSource {
                            empty_dir: Some(EmptyDirVolumeSource::default()),
                            ..Default::default()
                        }
                    }
                };

                Volume {
                    name: vol_name,
                    source,
                }
            })
            .collect()
    }

    fn translate_service_network(&self, name: &str, service: &ComposeService) -> Service {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), name.to_string());

        let mut selector = BTreeMap::new();
        selector.insert("app".to_string(), name.to_string());

        let ports: Vec<ServicePort> = service
            .ports
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let (host_port, container_port, protocol) = p.parse();
                ServicePort {
                    name: Some(format!("port-{}", i)),
                    port: host_port.unwrap_or(container_port) as i32,
                    target_port: Some(k1s_types::IntOrString::Int(container_port as i32)),
                    protocol: Some(match protocol.to_lowercase().as_str() {
                        "udp" => Protocol::Udp,
                        _ => Protocol::Tcp,
                    }),
                    node_port: host_port.map(|p| p as i32),
                    ..Default::default()
                }
            })
            .collect();

        // endpoint_mode: dnsrr → headless service (clusterIP: None)
        let endpoint_mode = service
            .deploy
            .as_ref()
            .and_then(|d| d.endpoint_mode.as_deref())
            .unwrap_or("vip");
        let headless = endpoint_mode == "dnsrr";

        // Determine service type
        let service_type = if headless {
            None // headless: no ClusterIP assigned
        } else if service.ports.iter().any(|p| {
            let (host, _, _) = p.parse();
            host.is_some()
        }) {
            Some(ServiceType::NodePort)
        } else {
            Some(ServiceType::ClusterIP)
        };

        let cluster_ip = if headless {
            Some("None".to_string())
        } else {
            None
        };

        Service {
            type_meta: TypeMeta::new("v1", "Service"),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(self.namespace.clone()),
                labels,
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                ports,
                selector,
                r#type: service_type,
                cluster_ip,
                ..Default::default()
            }),
            status: None,
        }
    }
}

/// Parse Swarm placement constraints into a nodeSelector map.
///
/// Supported constraint format: `node.labels.<key> == <value>`
/// Also handles: `node.role == manager/worker` → `node-role.kubernetes.io/<role>: ""`
fn parse_placement_constraints(constraints: &[String]) -> BTreeMap<String, String> {
    let mut selector = BTreeMap::new();
    for constraint in constraints {
        let parts: Vec<&str> = constraint.splitn(3, " == ").collect();
        if parts.len() == 2 {
            let key = parts[0].trim();
            let value = parts[1].trim().to_string();
            if key == "node.role" {
                selector.insert(
                    format!("node-role.kubernetes.io/{}", value),
                    String::new(),
                );
            } else if let Some(label_key) = key.strip_prefix("node.labels.") {
                selector.insert(label_key.to_string(), value);
            }
        }
    }
    selector
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_compose() {
        let yaml = r#"
version: "3"
services:
  web:
    image: nginx:latest
    ports:
      - "80:80"
    environment:
      - DEBUG=true
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let translator = ComposeTranslator::new("default", "test-app");
        let resources = translator.translate(&compose);

        assert_eq!(resources.pods.len(), 1);
        assert_eq!(resources.services.len(), 1);
        assert_eq!(resources.pods[0].metadata.name, "web");
    }

    #[test]
    fn test_swarm_replicated_service() {
        let yaml = r#"
version: "3.8"
services:
  api:
    image: myapp:latest
    ports:
      - "8080:8080"
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
        order: start-first
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let translator = ComposeTranslator::new("default", "test-stack");
        let resources = translator.translate(&compose);

        assert_eq!(resources.pods.len(), 0, "should emit Deployment, not Pod");
        assert_eq!(resources.deployments.len(), 1);
        assert_eq!(resources.daemon_sets.len(), 0);

        let deploy = &resources.deployments[0];
        assert_eq!(deploy.metadata.name, "api");
        let spec = deploy.spec.as_ref().unwrap();
        assert_eq!(spec.replicas, Some(3));
        assert!(spec.strategy.is_some());
    }

    #[test]
    fn test_swarm_global_service() {
        let yaml = r#"
version: "3.8"
services:
  agent:
    image: monitoring-agent:latest
    deploy:
      mode: global
      placement:
        constraints:
          - node.role == worker
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let translator = ComposeTranslator::new("default", "monitoring");
        let resources = translator.translate(&compose);

        assert_eq!(resources.pods.len(), 0);
        assert_eq!(resources.deployments.len(), 0);
        assert_eq!(resources.daemon_sets.len(), 1);

        let ds = &resources.daemon_sets[0];
        assert_eq!(ds.metadata.name, "agent");
        let spec = ds.spec.as_ref().unwrap();
        let node_sel = &spec.template.spec.as_ref().unwrap().node_selector;
        assert!(node_sel.contains_key("node-role.kubernetes.io/worker"));
    }

    #[test]
    fn test_swarm_dnsrr_headless_service() {
        let yaml = r#"
version: "3.8"
services:
  db:
    image: postgres:15
    ports:
      - "5432:5432"
    deploy:
      replicas: 3
      endpoint_mode: dnsrr
"#;
        let compose = ComposeFile::parse(yaml).unwrap();
        let translator = ComposeTranslator::new("default", "db-stack");
        let resources = translator.translate(&compose);

        assert_eq!(resources.services.len(), 1);
        let svc_spec = resources.services[0].spec.as_ref().unwrap();
        assert_eq!(svc_spec.cluster_ip.as_deref(), Some("None"));
    }

    #[test]
    fn test_placement_constraints_parse() {
        let constraints = vec![
            "node.role == manager".to_string(),
            "node.labels.region == us-east-1".to_string(),
        ];
        let selector = parse_placement_constraints(&constraints);
        assert!(selector.contains_key("node-role.kubernetes.io/manager"));
        assert_eq!(selector.get("region").map(String::as_str), Some("us-east-1"));
    }
}
