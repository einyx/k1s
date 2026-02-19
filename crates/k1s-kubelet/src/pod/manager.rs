//! Pod lifecycle manager

use std::sync::Arc;

use k1s_storage::SledBackend;
use k1s_types::{ContainerState, ContainerStateRunning, ContainerStatus, Pod, PodPhase, PodStatus};
use tracing::{debug, error, info};

use crate::runtime::{ContainerEnvConfig, ContainerRuntime};
use crate::secrets::SecretResolver;
use crate::{KubeletError, KubeletResult};

pub struct PodManager {
    runtime: Arc<dyn ContainerRuntime>,
    secret_resolver: SecretResolver,
}

impl PodManager {
    pub fn new(runtime: Arc<dyn ContainerRuntime>, storage: Arc<SledBackend>) -> Self {
        Self {
            runtime,
            secret_resolver: SecretResolver::new(storage),
        }
    }

    /// Sync pod state - create/update containers to match spec
    pub async fn sync_pod(&self, pod: &mut Pod) -> KubeletResult<()> {
        let spec = pod.spec.as_ref().ok_or_else(|| {
            KubeletError::Pod("Pod has no spec".to_string())
        })?;

        info!("Syncing pod {}/{}", pod.metadata.effective_namespace(), pod.metadata.name);

        // Initialize status if not present
        if pod.status.is_none() {
            pod.status = Some(PodStatus {
                phase: Some(PodPhase::Pending),
                ..Default::default()
            });
        }

        let mut container_statuses = Vec::new();

        // Process init containers first
        for container in &spec.init_containers {
            // TODO: Run init containers sequentially
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

            let container_id = if container_exists {
                existing
                    .iter()
                    .find(|c| c.name.contains(&container.name))
                    .map(|c| c.id.clone())
                    .unwrap_or_default()
            } else {
                // Create container with resolved environment
                match self.runtime.create_container_with_env(pod, &container.name, &env_config).await {
                    Ok(id) => {
                        // Start container
                        if let Err(e) = self.runtime.start_container(&id).await {
                            error!("Failed to start container: {}", e);
                        }
                        id
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

                container_statuses.push(ContainerStatus {
                    name: container.name.clone(),
                    ready: matches!(info.state, crate::runtime::ContainerState::Running),
                    container_id: Some(format!("docker://{}", container_id)),
                    image: Some(container.image.clone()),
                    state: Some(state),
                    ..Default::default()
                });
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

    /// Delete a pod and its containers
    pub async fn delete_pod(&self, pod: &Pod) -> KubeletResult<()> {
        info!("Deleting pod {}/{}", pod.metadata.effective_namespace(), pod.metadata.name);

        let containers = self.runtime.list_containers(&pod.metadata.uid).await?;

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

        Ok(())
    }
}
