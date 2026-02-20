//! Deployment controller
//!
//! Manages ReplicaSets for rolling updates and declarative pod management.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    Deployment, DeploymentStatus, DeploymentStrategyType, LabelSelector, ObjectMeta,
    OwnerReference, PodTemplateSpec, ReplicaSet, ReplicaSetSpec, TypeMeta,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct DeploymentController {
    storage: Arc<SledBackend>,
}

impl DeploymentController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Generate a hash for a pod template spec (simplified version)
    fn template_hash(template: &PodTemplateSpec) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash the container images and names
        if let Some(spec) = &template.spec {
            for container in &spec.containers {
                container.name.hash(&mut hasher);
                container.image.hash(&mut hasher);
            }
        }
        // Hash labels
        for (k, v) in &template.metadata.labels {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        let hash = hasher.finish();
        format!("{:x}", hash)[..10].to_string()
    }

    /// Check if a ReplicaSet is owned by the given Deployment
    fn is_owned_by(rs: &ReplicaSet, deployment: &Deployment) -> bool {
        rs.metadata.owner_references.iter().any(|owner| {
            owner.kind == "Deployment"
                && owner.name == deployment.metadata.name
                && owner.uid == deployment.metadata.uid
        })
    }

    /// Create a ReplicaSet from Deployment template
    fn create_replicaset_from_deployment(deployment: &Deployment) -> ReplicaSet {
        let spec = deployment
            .spec
            .as_ref()
            .expect("Deployment must have spec");
        let namespace = deployment
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // Generate template hash for ReplicaSet name
        let template_hash = Self::template_hash(&spec.template);
        let rs_name = format!("{}-{}", deployment.metadata.name, template_hash);

        // Merge labels: template labels + pod-template-hash
        let mut labels = spec.template.metadata.labels.clone();
        labels.insert("pod-template-hash".to_string(), template_hash.clone());

        // Ensure selector labels are present in template
        for (k, v) in &spec.selector.match_labels {
            labels.insert(k.clone(), v.clone());
        }

        // Create selector that includes pod-template-hash
        let mut selector_labels = spec.selector.match_labels.clone();
        selector_labels.insert("pod-template-hash".to_string(), template_hash);

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            name: deployment.metadata.name.clone(),
            uid: deployment.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        // Create pod template with updated labels
        let mut pod_template = spec.template.clone();
        pod_template.metadata.labels = labels;

        ReplicaSet {
            type_meta: TypeMeta::new("apps/v1", "ReplicaSet"),
            metadata: ObjectMeta {
                name: rs_name,
                namespace: Some(namespace),
                labels: selector_labels.clone(),
                annotations: deployment.metadata.annotations.clone(),
                owner_references: vec![owner_ref],
                ..Default::default()
            },
            spec: Some(ReplicaSetSpec {
                replicas: spec.replicas,
                selector: LabelSelector {
                    match_labels: selector_labels,
                    match_expressions: Vec::new(),
                },
                template: pod_template,
                min_ready_seconds: spec.min_ready_seconds,
            }),
            status: None,
        }
    }

    /// Check if the current ReplicaSet needs to be updated
    fn needs_new_replicaset(deployment: &Deployment, rs: &ReplicaSet) -> bool {
        let spec = match &deployment.spec {
            Some(s) => s,
            None => return false,
        };

        let current_hash = Self::template_hash(&spec.template);
        let rs_hash = rs
            .metadata
            .labels
            .get("pod-template-hash")
            .cloned()
            .unwrap_or_default();

        current_hash != rs_hash
    }

    /// Scale a ReplicaSet to desired replicas
    async fn scale_replicaset(
        &self,
        rs_store: &ResourceStore<ReplicaSet>,
        rs: &ReplicaSet,
        replicas: i32,
    ) -> ControllerResult<()> {
        let mut updated_rs = rs.clone();
        if let Some(spec) = &mut updated_rs.spec {
            spec.replicas = Some(replicas);
        }
        rs_store.update(updated_rs).await?;
        Ok(())
    }
}

#[async_trait]
impl Controller for DeploymentController {
    fn name(&self) -> &str {
        "deployment"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let deployment_store = ResourceStore::<Deployment>::new(self.storage.clone());
        let rs_store = ResourceStore::<ReplicaSet>::new(self.storage.clone());

        let deployments = deployment_store.list(None).await?;

        for deployment in deployments {
            let namespace = deployment.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &deployment.spec {
                Some(s) => s,
                None => continue,
            };

            let desired_replicas = spec.replicas.unwrap_or(1);

            // Find ReplicaSets owned by this Deployment
            let all_rs = rs_store.list(Some(namespace)).await?;
            let owned_rs: Vec<&ReplicaSet> = all_rs
                .iter()
                .filter(|rs| Self::is_owned_by(rs, &deployment))
                .collect();

            debug!(
                "Deployment {}/{}: found {} owned ReplicaSets",
                namespace,
                deployment.metadata.name,
                owned_rs.len()
            );

            // Find the "current" ReplicaSet (matching template hash)
            let current_hash = Self::template_hash(&spec.template);
            let current_rs = owned_rs.iter().find(|rs| {
                rs.metadata
                    .labels
                    .get("pod-template-hash")
                    .map(|h| h == &current_hash)
                    .unwrap_or(false)
            });

            // If no current RS exists, create one
            if current_rs.is_none() {
                info!(
                    "Creating new ReplicaSet for Deployment {}/{}",
                    namespace, deployment.metadata.name
                );

                let new_rs = Self::create_replicaset_from_deployment(&deployment);
                match rs_store.create(new_rs).await {
                    Ok(created) => {
                        info!(
                            "Created ReplicaSet {}/{} for Deployment {}",
                            namespace, created.metadata.name, deployment.metadata.name
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Failed to create ReplicaSet for Deployment {}/{}: {}",
                            namespace, deployment.metadata.name, e
                        );
                    }
                }

                // Scale down old ReplicaSets
                for old_rs in &owned_rs {
                    let strategy_type = spec
                        .strategy
                        .as_ref()
                        .and_then(|s| s.r#type.as_ref())
                        .unwrap_or(&DeploymentStrategyType::RollingUpdate);

                    match strategy_type {
                        DeploymentStrategyType::Recreate => {
                            // Scale to 0 immediately
                            if old_rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0) > 0 {
                                info!(
                                    "Scaling down old ReplicaSet {}/{} to 0 (Recreate strategy)",
                                    namespace, old_rs.metadata.name
                                );
                                self.scale_replicaset(&rs_store, old_rs, 0).await?;
                            }
                        }
                        DeploymentStrategyType::RollingUpdate => {
                            // Gradually scale down (will happen in subsequent reconciliation loops)
                            // For now, scale down to 0 after current RS is ready
                            let old_replicas =
                                old_rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
                            if old_replicas > 0 {
                                info!(
                                    "Scaling down old ReplicaSet {}/{} to 0 (RollingUpdate)",
                                    namespace, old_rs.metadata.name
                                );
                                self.scale_replicaset(&rs_store, old_rs, 0).await?;
                            }
                        }
                    }
                }
            } else {
                // Current RS exists, ensure it has correct replica count
                let current_rs = current_rs.unwrap();
                let current_replicas = current_rs
                    .spec
                    .as_ref()
                    .and_then(|s| s.replicas)
                    .unwrap_or(0);

                if current_replicas != desired_replicas {
                    info!(
                        "Scaling ReplicaSet {}/{} from {} to {} replicas",
                        namespace, current_rs.metadata.name, current_replicas, desired_replicas
                    );
                    self.scale_replicaset(&rs_store, current_rs, desired_replicas)
                        .await?;
                }

                // Scale down old ReplicaSets
                for old_rs in &owned_rs {
                    if old_rs.metadata.name == current_rs.metadata.name {
                        continue;
                    }

                    let old_replicas = old_rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
                    if old_replicas > 0 {
                        info!(
                            "Scaling down old ReplicaSet {}/{} to 0",
                            namespace, old_rs.metadata.name
                        );
                        self.scale_replicaset(&rs_store, old_rs, 0).await?;
                    }
                }
            }

            // Update Deployment status
            let total_replicas: i32 = owned_rs
                .iter()
                .map(|rs| rs.status.as_ref().map(|s| s.replicas).unwrap_or(0))
                .sum();
            let ready_replicas: i32 = owned_rs
                .iter()
                .map(|rs| rs.status.as_ref().map(|s| s.ready_replicas).unwrap_or(0))
                .sum();
            let available_replicas: i32 = owned_rs
                .iter()
                .map(|rs| {
                    rs.status
                        .as_ref()
                        .map(|s| s.available_replicas)
                        .unwrap_or(0)
                })
                .sum();
            let updated_replicas = current_rs
                .map(|rs| rs.status.as_ref().map(|s| s.replicas).unwrap_or(0))
                .unwrap_or(0);

            let current_status = deployment.status.clone().unwrap_or_default();
            if current_status.replicas != total_replicas
                || current_status.ready_replicas != ready_replicas
                || current_status.available_replicas != available_replicas
                || current_status.updated_replicas != updated_replicas
            {
                let mut updated_deployment = deployment.clone();
                updated_deployment.status = Some(DeploymentStatus {
                    replicas: total_replicas,
                    ready_replicas,
                    available_replicas,
                    updated_replicas,
                });

                if let Err(e) = deployment_store.update(updated_deployment).await {
                    warn!(
                        "Failed to update Deployment {}/{} status: {}",
                        namespace, deployment.metadata.name, e
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1s_types::PodSpec;

    #[test]
    fn test_template_hash_consistency() {
        let template = PodTemplateSpec {
            metadata: ObjectMeta {
                labels: {
                    let mut m = BTreeMap::new();
                    m.insert("app".to_string(), "nginx".to_string());
                    m
                },
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![k1s_types::Container {
                    name: "nginx".to_string(),
                    image: "nginx:latest".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        };

        let hash1 = DeploymentController::template_hash(&template);
        let hash2 = DeploymentController::template_hash(&template);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 10);
    }
}
