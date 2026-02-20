//! Pod lifecycle manager

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use k1s_networking::cni::{BridgeCni, BridgeConfig, CniDns, CniIp, CniResult, CniRoute, DockerBridgeCni, DockerBridgeConfig};
use k1s_storage::SledBackend;
use k1s_types::{ContainerStateRunning, ContainerStatus, Pod, PodCondition, PodPhase, PodStatus};
use tracing::{debug, error, info, warn};
use std::net::Ipv4Addr;

use crate::runtime::{ContainerEnvConfig, ContainerRuntime};
use crate::secrets::SecretResolver;
use crate::{KubeletError, KubeletResult};

use super::probes::{ProbeExecutor, ProbeState};

/// CNI implementation - either Linux native or Docker-based
enum CniImpl {
    /// Linux native CNI with network namespaces
    Linux(Arc<BridgeCni>),
    /// Docker-based CNI for macOS and other platforms
    Docker(Arc<DockerBridgeCni>),
}

pub struct PodManager {
    runtime: Arc<dyn ContainerRuntime>,
    secret_resolver: SecretResolver,
    probe_executor: ProbeExecutor,
    /// Probe states per container (pod_uid/container_name -> (liveness_state, readiness_state, startup_state))
    probe_states: RwLock<HashMap<String, (ProbeState, ProbeState, ProbeState)>>,
    /// CNI implementation
    cni: CniImpl,
}

impl PodManager {
    pub fn new(runtime: Arc<dyn ContainerRuntime>, storage: Arc<SledBackend>) -> Self {
        Self::with_cni_config(runtime, storage, BridgeConfig::default())
    }

    pub fn with_cni_config(
        runtime: Arc<dyn ContainerRuntime>,
        storage: Arc<SledBackend>,
        cni_config: BridgeConfig,
    ) -> Self {
        let probe_executor = ProbeExecutor::new(runtime.clone());

        // Use Docker CNI on non-Linux platforms, Linux native CNI on Linux
        let cni = if cfg!(target_os = "linux") {
            let bridge_cni = Arc::new(BridgeCni::new(cni_config));
            if let Err(e) = bridge_cni.init() {
                warn!("Failed to initialize Linux CNI bridge: {}", e);
            }
            CniImpl::Linux(bridge_cni)
        } else {
            // Use Docker-based CNI on macOS/Windows
            let docker_config = DockerBridgeConfig::from(cni_config);
            match DockerBridgeCni::new(docker_config) {
                Ok(docker_cni) => {
                    info!("Using Docker-based CNI for pod networking");
                    CniImpl::Docker(Arc::new(docker_cni))
                }
                Err(e) => {
                    warn!("Failed to initialize Docker CNI, falling back to Linux CNI: {}", e);
                    let bridge_cni = Arc::new(BridgeCni::new(BridgeConfig::default()));
                    CniImpl::Linux(bridge_cni)
                }
            }
        };

        Self {
            runtime,
            secret_resolver: SecretResolver::new(storage),
            probe_executor,
            probe_states: RwLock::new(HashMap::new()),
            cni,
        }
    }

    /// Initialize CNI (async for Docker CNI)
    pub async fn init_cni(&self) -> KubeletResult<()> {
        match &self.cni {
            CniImpl::Linux(_) => {
                // Already initialized in constructor
                Ok(())
            }
            CniImpl::Docker(docker_cni) => {
                docker_cni.init().await.map_err(|e| {
                    KubeletError::Runtime(format!("Failed to initialize Docker CNI: {}", e))
                })
            }
        }
    }

    /// Sync pod state - create/update containers to match spec
    pub async fn sync_pod(&self, pod: &mut Pod) -> KubeletResult<()> {
        // Check spec existence first
        if pod.spec.is_none() {
            return Err(KubeletError::Pod("Pod has no spec".to_string()));
        }

        info!("Syncing pod {}/{}", pod.metadata.effective_namespace(), pod.metadata.name);

        // Initialize status if not present
        if pod.status.is_none() {
            pod.status = Some(PodStatus {
                phase: Some(PodPhase::Pending),
                ..Default::default()
            });
        }

        // Set up pod networking with CNI if not already done
        if pod.status.as_ref().and_then(|s| s.pod_ip.as_ref()).is_none() {
            self.setup_pod_network(pod).await?;
        }

        // Now get spec reference for processing containers
        let spec = pod.spec.as_ref().unwrap();

        let mut container_statuses = Vec::new();
        let mut init_container_statuses = Vec::new();

        // Process init containers first - run sequentially, wait for each to complete
        for container in &spec.init_containers {
            info!("Running init container {} for pod {}", container.name, pod.metadata.name);

            // Pull image if needed
            if let Err(e) = self.runtime.pull_image(&container.image).await {
                error!("Failed to pull init container image {}: {}", container.image, e);
                init_container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: false,
                    state: Some(k1s_types::ContainerState {
                        waiting: Some(k1s_types::ContainerStateWaiting {
                            reason: Some("ImagePullBackOff".to_string()),
                            message: Some(e.to_string()),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                // Update pod status to show init container failure
                if let Some(status) = &mut pod.status {
                    status.init_container_statuses = init_container_statuses.clone();
                    status.phase = Some(PodPhase::Pending);
                }
                return Ok(()); // Stop processing, init failed
            }

            // Resolve ConfigMaps and Secrets for init container
            let env_config = match self.secret_resolver.resolve_container_env(pod, container).await {
                Ok(resolved) => ContainerEnvConfig {
                    env_vars: resolved.env_vars,
                    volume_data: resolved.volume_data,
                },
                Err(e) => {
                    error!("Failed to resolve env for init container {}: {}", container.name, e);
                    init_container_statuses.push(ContainerStatus {
                        name: container.name.clone(),
                        ready: false,
                        state: Some(k1s_types::ContainerState {
                            waiting: Some(k1s_types::ContainerStateWaiting {
                                reason: Some("CreateContainerConfigError".to_string()),
                                message: Some(e.to_string()),
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    if let Some(status) = &mut pod.status {
                        status.init_container_statuses = init_container_statuses.clone();
                        status.phase = Some(PodPhase::Pending);
                    }
                    return Ok(());
                }
            };

            // Create and start init container
            let container_id = match self.runtime.create_container_with_env(pod, &container.name, &env_config).await {
                Ok(id) => id,
                Err(e) => {
                    error!("Failed to create init container {}: {}", container.name, e);
                    init_container_statuses.push(ContainerStatus {
                        name: container.name.clone(),
                        ready: false,
                        state: Some(k1s_types::ContainerState {
                            waiting: Some(k1s_types::ContainerStateWaiting {
                                reason: Some("ContainerCreating".to_string()),
                                message: Some(e.to_string()),
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    if let Some(status) = &mut pod.status {
                        status.init_container_statuses = init_container_statuses.clone();
                        status.phase = Some(PodPhase::Pending);
                    }
                    return Ok(());
                }
            };

            if let Err(e) = self.runtime.start_container(&container_id).await {
                error!("Failed to start init container {}: {}", container.name, e);
                init_container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: false,
                    container_id: Some(format!("docker://{}", container_id)),
                    state: Some(k1s_types::ContainerState {
                        waiting: Some(k1s_types::ContainerStateWaiting {
                            reason: Some("CrashLoopBackOff".to_string()),
                            message: Some(e.to_string()),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                if let Some(status) = &mut pod.status {
                    status.init_container_statuses = init_container_statuses.clone();
                    status.phase = Some(PodPhase::Pending);
                }
                return Ok(());
            }

            // Wait for init container to complete (poll for up to 5 minutes)
            let mut completed = false;
            let mut exit_code = 0;
            for _ in 0..300 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                if let Ok(Some(info)) = self.runtime.get_container(&container_id).await {
                    if matches!(info.state, crate::runtime::ContainerState::Stopped) {
                        completed = true;
                        exit_code = info.exit_code.unwrap_or(0);
                        break;
                    }
                }
            }

            if !completed {
                warn!("Init container {} timed out", container.name);
                init_container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: false,
                    container_id: Some(format!("docker://{}", container_id)),
                    state: Some(k1s_types::ContainerState {
                        running: Some(ContainerStateRunning {
                            started_at: Some(chrono::Utc::now()),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                if let Some(status) = &mut pod.status {
                    status.init_container_statuses = init_container_statuses.clone();
                    status.phase = Some(PodPhase::Pending);
                }
                return Ok(());
            }

            if exit_code != 0 {
                error!("Init container {} failed with exit code {}", container.name, exit_code);
                init_container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: false,
                    container_id: Some(format!("docker://{}", container_id)),
                    state: Some(k1s_types::ContainerState {
                        terminated: Some(k1s_types::ContainerStateTerminated {
                            exit_code,
                            reason: Some("Error".to_string()),
                            finished_at: Some(chrono::Utc::now()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                if let Some(status) = &mut pod.status {
                    status.init_container_statuses = init_container_statuses.clone();
                    status.phase = Some(PodPhase::Failed);
                }
                return Ok(());
            }

            // Init container completed successfully
            info!("Init container {} completed successfully", container.name);
            init_container_statuses.push(ContainerStatus {
                name: container.name.clone(),
                ready: true,
                container_id: Some(format!("docker://{}", container_id)),
                state: Some(k1s_types::ContainerState {
                    terminated: Some(k1s_types::ContainerStateTerminated {
                        exit_code: 0,
                        reason: Some("Completed".to_string()),
                        finished_at: Some(chrono::Utc::now()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            });

            // Clean up init container
            let _ = self.runtime.remove_container(&container_id).await;
        }

        // Update init container statuses
        if let Some(status) = &mut pod.status {
            status.init_container_statuses = init_container_statuses;
        }

        // Process regular containers
        for container in &spec.containers {
            // Pull image if needed
            if let Err(e) = self.runtime.pull_image(&container.image).await {
                error!("Failed to pull image {}: {}", container.image, e);
                container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: false,
                    state: Some(k1s_types::ContainerState {
                        waiting: Some(k1s_types::ContainerStateWaiting {
                            reason: Some("ImagePullBackOff".to_string()),
                            message: Some(e.to_string()),
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                });
                continue;
            }

            // Resolve ConfigMaps and Secrets for this container
            let env_config = match self.secret_resolver.resolve_container_env(pod, container).await {
                Ok(resolved) => {
                    debug!(
                        "Resolved {} env vars and {} volume mounts for container {}",
                        resolved.env_vars.len(),
                        resolved.volume_data.len(),
                        container.name
                    );
                    ContainerEnvConfig {
                        env_vars: resolved.env_vars,
                        volume_data: resolved.volume_data,
                    }
                }
                Err(e) => {
                    error!("Failed to resolve ConfigMaps/Secrets for container {}: {}", container.name, e);
                    container_statuses.push(ContainerStatus {
                        name: container.name.clone(),
                        ready: false,
                        state: Some(k1s_types::ContainerState {
                            waiting: Some(k1s_types::ContainerStateWaiting {
                                reason: Some("CreateContainerConfigError".to_string()),
                                message: Some(e.to_string()),
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    continue;
                }
            };

            // Check if container exists
            let existing = self.runtime.list_containers(&pod.metadata.uid).await?;
            let container_exists = existing.iter().any(|c| c.name.contains(&container.name));

            let (container_id, is_new) = if container_exists {
                let id = existing
                    .iter()
                    .find(|c| c.name.contains(&container.name))
                    .map(|c| c.id.clone())
                    .unwrap_or_default();
                (id, false)
            } else {
                // Create container with resolved environment
                match self.runtime.create_container_with_env(pod, &container.name, &env_config).await {
                    Ok(id) => {
                        // Start container
                        if let Err(e) = self.runtime.start_container(&id).await {
                            error!("Failed to start container: {}", e);
                        }

                        // Connect container to pod network (for Docker CNI)
                        // We'll update pod IP after the loop to avoid borrow issues
                        let _ = self.connect_container_to_network_inner(&pod.metadata.uid, &id).await;

                        (id, true)
                    }
                    Err(e) => {
                        error!("Failed to create container: {}", e);
                        container_statuses.push(ContainerStatus {
                            name: container.name.clone(),
                            ready: false,
                            state: Some(k1s_types::ContainerState {
                                waiting: Some(k1s_types::ContainerStateWaiting {
                                    reason: Some("ContainerCreating".to_string()),
                                    message: Some(e.to_string()),
                                }),
                                ..Default::default()
                            }),
                            ..Default::default()
                        });
                        continue;
                    }
                }
            };

            // Suppress unused variable warning
            let _ = is_new;

            // Get container status
            if let Some(info) = self.runtime.get_container(&container_id).await? {
                let state = match info.state {
                    crate::runtime::ContainerState::Running => k1s_types::ContainerState {
                        running: Some(ContainerStateRunning {
                            started_at: Some(chrono::Utc::now()),
                        }),
                        ..Default::default()
                    },
                    crate::runtime::ContainerState::Stopped => k1s_types::ContainerState {
                        terminated: Some(k1s_types::ContainerStateTerminated {
                            exit_code: info.exit_code.unwrap_or(0),
                            finished_at: Some(chrono::Utc::now()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    _ => k1s_types::ContainerState {
                        waiting: Some(k1s_types::ContainerStateWaiting {
                            reason: Some("ContainerCreating".to_string()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                };

                // Check if container is running and run probes
                let mut is_ready = matches!(info.state, crate::runtime::ContainerState::Running);

                if is_ready {
                    // Get pod IP for probe execution
                    let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.as_deref());

                    // Run probes if container is running
                    let probe_key = format!("{}/{}", pod.metadata.uid, container.name);

                    // Get or create probe state
                    let mut probe_states = self.probe_states.write().await;
                    let (liveness_state, readiness_state, startup_state) = probe_states
                        .entry(probe_key.clone())
                        .or_insert_with(|| (ProbeState::default(), ProbeState::default(), ProbeState::default()));

                    // Check startup probe first (if defined and not yet passed)
                    if let Some(startup_probe) = &container.startup_probe {
                        if !startup_state.has_succeeded {
                            let result = self.probe_executor
                                .execute_probe(startup_probe, &container_id, pod_ip)
                                .await;
                            startup_state.record_result(result);

                            if startup_state.is_failed(startup_probe.failure_threshold) {
                                warn!("Startup probe failed for container {}", container.name);
                                is_ready = false;
                            } else if !startup_state.has_succeeded {
                                // Startup probe hasn't succeeded yet, container not ready
                                is_ready = false;
                            }
                        }
                    }

                    // Only run readiness probe if startup has passed (or no startup probe)
                    let startup_passed = container.startup_probe.is_none() || startup_state.has_succeeded;

                    if startup_passed {
                        // Check readiness probe
                        if let Some(readiness_probe) = &container.readiness_probe {
                            let result = self.probe_executor
                                .execute_probe(readiness_probe, &container_id, pod_ip)
                                .await;
                            readiness_state.record_result(result);

                            is_ready = readiness_state.is_successful(readiness_probe.success_threshold);
                        }

                        // Check liveness probe (failure means container should be restarted)
                        if let Some(liveness_probe) = &container.liveness_probe {
                            let result = self.probe_executor
                                .execute_probe(liveness_probe, &container_id, pod_ip)
                                .await;
                            liveness_state.record_result(result);

                            if liveness_state.is_failed(liveness_probe.failure_threshold) {
                                warn!("Liveness probe failed for container {}, restarting", container.name);

                                // Restart the container
                                if let Err(e) = self.restart_container(pod, container, &container_id).await {
                                    error!("Failed to restart container {}: {}", container.name, e);
                                } else {
                                    info!("Successfully restarted container {}", container.name);
                                    // Reset probe state after restart
                                    liveness_state.reset();
                                }
                            }
                        }
                    }

                    drop(probe_states); // Release write lock
                }

                container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: is_ready,
                    container_id: Some(format!("docker://{}", container_id)),
                    image: Some(container.image.clone()),
                    state: Some(state),
                    ..Default::default()
                });
            }
        }

        // For Docker CNI, update pod IP if not already set
        if let CniImpl::Docker(cni) = &self.cni {
            if pod.status.as_ref().and_then(|s| s.pod_ip.as_ref()).is_none() {
                if let Some(ip) = cni.get_ip(&pod.metadata.uid) {
                    let pod_ip = ip.to_string();
                    if let Some(status) = &mut pod.status {
                        status.pod_ip = Some(pod_ip.clone());
                        status.pod_ips = vec![k1s_types::PodIP { ip: pod_ip }];
                    }
                }
            }
        }

        // Update pod status
        if let Some(status) = &mut pod.status {
            status.container_statuses = container_statuses.clone();

            // Determine pod phase
            let all_running = container_statuses.iter().all(|s| s.ready);
            let any_failed = container_statuses.iter().any(|s| {
                s.state.as_ref().map_or(false, |state| {
                    state.terminated.as_ref().map_or(false, |t| t.exit_code != 0)
                })
            });

            status.phase = Some(if any_failed {
                PodPhase::Failed
            } else if all_running {
                PodPhase::Running
            } else {
                PodPhase::Pending
            });
        }

        Ok(())
    }

    /// Set up pod networking using CNI
    async fn setup_pod_network(&self, pod: &mut Pod) -> KubeletResult<()> {
        let pod_uid = &pod.metadata.uid;

        info!(
            "Setting up network for pod {}/{} (uid: {})",
            pod.metadata.effective_namespace(),
            pod.metadata.name,
            pod_uid
        );

        let result = match &self.cni {
            CniImpl::Linux(cni) => {
                // Linux CNI with network namespaces
                let netns_path = format!("/var/run/netns/{}", pod_uid);
                cni.add(pod_uid, &netns_path, "eth0")
            }
            CniImpl::Docker(cni) => {
                // For Docker CNI, we pre-allocate an IP
                // The actual network connection happens after container creation
                // For now, just allocate and return a result
                let ip = cni.get_ip(pod_uid);
                if let Some(ip) = ip {
                    // Already allocated
                    Ok(cni_result_from_ip(ip, "10.42.0.1"))
                } else {
                    // Will be allocated when we connect the first container
                    // Return a pending result
                    Ok(k1s_networking::cni::CniResult {
                        ips: vec![],
                        routes: vec![],
                        dns: k1s_networking::cni::CniDns::default(),
                    })
                }
            }
        };

        match result {
            Ok(result) => {
                if !result.ips.is_empty() {
                    // Extract pod IP from CNI result
                    let pod_ip = result.ips.first().map(|ip| {
                        ip.address.split('/').next().unwrap_or(&ip.address).to_string()
                    });

                    // Update pod status with network info
                    if let Some(status) = &mut pod.status {
                        status.pod_ip = pod_ip.clone();
                        status.pod_ips = result
                            .ips
                            .iter()
                            .map(|ip| k1s_types::PodIP {
                                ip: ip.address.split('/').next().unwrap_or(&ip.address).to_string(),
                            })
                            .collect();

                        // Add conditions
                        status.conditions.push(PodCondition {
                            r#type: "PodScheduled".to_string(),
                            status: "True".to_string(),
                            reason: Some("PodScheduled".to_string()),
                            message: Some("Pod has been scheduled and network is ready".to_string()),
                            last_probe_time: None,
                            last_transition_time: Some(chrono::Utc::now()),
                        });
                        status.conditions.push(PodCondition {
                            r#type: "Initialized".to_string(),
                            status: "True".to_string(),
                            reason: Some("PodHasNetwork".to_string()),
                            message: Some("Pod network has been configured".to_string()),
                            last_probe_time: None,
                            last_transition_time: Some(chrono::Utc::now()),
                        });
                    }

                    info!(
                        "Pod {}/{} assigned IP: {}",
                        pod.metadata.effective_namespace(),
                        pod.metadata.name,
                        pod_ip.unwrap_or_else(|| "none".to_string())
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to set up network for pod {}/{}: {}",
                    pod.metadata.effective_namespace(),
                    pod.metadata.name,
                    e
                );
            }
        }

        Ok(())
    }

    /// Connect a container to the pod network (for Docker CNI)
    /// Returns the assigned IP address if successful
    async fn connect_container_to_network_inner(
        &self,
        pod_uid: &str,
        docker_container_id: &str,
    ) -> Option<String> {
        if let CniImpl::Docker(cni) = &self.cni {
            match cni.add(pod_uid, docker_container_id).await {
                Ok(result) => {
                    if let Some(ip_info) = result.ips.first() {
                        let pod_ip = ip_info.address.split('/').next().unwrap_or(&ip_info.address).to_string();
                        info!(
                            "Connected container {} to network with IP {}",
                            docker_container_id, pod_ip
                        );
                        return Some(pod_ip);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to connect container {} to network: {}",
                        docker_container_id, e
                    );
                }
            }
        }

        None
    }

    /// Clean up pod networking using CNI
    async fn cleanup_pod_network(&self, pod: &Pod, docker_container_ids: &[String]) {
        let pod_uid = &pod.metadata.uid;

        info!(
            "Cleaning up network for pod {}/{}",
            pod.metadata.effective_namespace(),
            pod.metadata.name
        );

        match &self.cni {
            CniImpl::Linux(cni) => {
                let netns_path = format!("/var/run/netns/{}", pod_uid);
                if let Err(e) = cni.del(pod_uid, &netns_path, "eth0") {
                    warn!(
                        "Failed to clean up network for pod {}/{}: {}",
                        pod.metadata.effective_namespace(),
                        pod.metadata.name,
                        e
                    );
                }
            }
            CniImpl::Docker(cni) => {
                // Disconnect all containers from the network
                for container_id in docker_container_ids {
                    if let Err(e) = cni.del(pod_uid, container_id).await {
                        warn!(
                            "Failed to disconnect container {} from network: {}",
                            container_id, e
                        );
                    }
                }
            }
        }
    }

    /// Delete a pod and its containers
    pub async fn delete_pod(&self, pod: &Pod) -> KubeletResult<()> {
        info!("Deleting pod {}/{}", pod.metadata.effective_namespace(), pod.metadata.name);

        let containers = self.runtime.list_containers(&pod.metadata.uid).await?;
        let container_ids: Vec<String> = containers.iter().map(|c| c.id.clone()).collect();

        for container in containers {
            // Stop container
            if let Err(e) = self.runtime.stop_container(&container.id, 30).await {
                error!("Failed to stop container {}: {}", container.id, e);
            }

            // Remove container
            if let Err(e) = self.runtime.remove_container(&container.id).await {
                error!("Failed to remove container {}: {}", container.id, e);
            }
        }

        // Clean up CNI networking
        self.cleanup_pod_network(pod, &container_ids).await;

        Ok(())
    }

    /// Restart a container (stop, remove, recreate, start)
    async fn restart_container(
        &self,
        pod: &Pod,
        container: &k1s_types::Container,
        container_id: &str,
    ) -> KubeletResult<()> {
        info!("Restarting container {} ({})", container.name, container_id);

        // Stop the container with a timeout
        self.runtime.stop_container(container_id, 10).await?;

        // Remove the container
        self.runtime.remove_container(container_id).await?;

        // Resolve ConfigMaps and Secrets for the container
        let env_config = match self.secret_resolver.resolve_container_env(pod, container).await {
            Ok(resolved) => ContainerEnvConfig {
                env_vars: resolved.env_vars,
                volume_data: resolved.volume_data,
            },
            Err(e) => {
                error!("Failed to resolve ConfigMaps/Secrets during restart: {}", e);
                ContainerEnvConfig::default()
            }
        };

        // Create a new container
        let new_container_id = self.runtime
            .create_container_with_env(pod, &container.name, &env_config)
            .await?;

        // Start the new container
        self.runtime.start_container(&new_container_id).await?;

        info!("Container {} restarted successfully with new ID {}", container.name, new_container_id);
        Ok(())
    }
}

/// Helper to create a CniResult from an allocated IP
fn cni_result_from_ip(ip: Ipv4Addr, gateway: &str) -> CniResult {
    CniResult {
        ips: vec![CniIp {
            address: format!("{}/24", ip),
            gateway: Some(gateway.to_string()),
            interface: Some(0),
        }],
        routes: vec![CniRoute {
            dst: "0.0.0.0/0".to_string(),
            gw: Some(gateway.to_string()),
        }],
        dns: CniDns {
            nameservers: vec!["10.43.0.10".to_string()],
            domain: Some("cluster.local".to_string()),
            search: vec![
                "default.svc.cluster.local".to_string(),
                "svc.cluster.local".to_string(),
                "cluster.local".to_string(),
            ],
            options: vec![],
        },
    }
}
