//! ReplicaSet controller
//!
//! Manages pod replicas to match the desired count specified in ReplicaSet resources.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{ObjectMeta, OwnerReference, Pod, PodPhase, ReplicaSet, ReplicaSetStatus, TypeMeta};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct ReplicaSetController {
    storage: Arc<SledBackend>,
}

impl ReplicaSetController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a pod is owned by the given ReplicaSet
    fn is_owned_by(pod: &Pod, rs: &ReplicaSet) -> bool {
        pod.metadata.owner_references.iter().any(|owner| {
            owner.kind == "ReplicaSet"
                && owner.name == rs.metadata.name
                && owner.uid == rs.metadata.uid
        })
    }

    /// Check if pod labels match the ReplicaSet selector
    fn labels_match_selector(
        labels: &BTreeMap<String, String>,
        selector: &BTreeMap<String, String>,
    ) -> bool {
        selector.iter().all(|(k, v)| labels.get(k) == Some(v))
    }

    /// Create a pod from ReplicaSet template
    fn create_pod_from_template(rs: &ReplicaSet) -> Pod {
        let spec = rs.spec.as_ref().expect("ReplicaSet must have spec");
        let template = &spec.template;
        let namespace = rs
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // Generate unique pod name
        let pod_name = format!("{}-{}", rs.metadata.name, generate_suffix());

        // Get labels from template, ensure selector labels are present
        let mut labels = template.metadata.labels.clone();
        for (k, v) in &spec.selector.match_labels {
            labels.insert(k.clone(), v.clone());
        }

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "apps/v1".to_string(),
            kind: "ReplicaSet".to_string(),
            name: rs.metadata.name.clone(),
            uid: rs.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        Pod {
            type_meta: TypeMeta::new("v1", "Pod"),
            metadata: ObjectMeta {
                name: pod_name,
                namespace: Some(namespace),
                labels,
                annotations: template.metadata.annotations.clone(),
                owner_references: vec![owner_ref],
                ..Default::default()
            },
            spec: template.spec.clone(),
            status: None,
        }
    }

    /// Check if a pod is considered ready
    fn pod_is_ready(pod: &Pod) -> bool {
        if let Some(status) = &pod.status {
            // Check for Ready condition
            for condition in &status.conditions {
                if condition.r#type == "Ready" && condition.status == "True" {
                    return true;
                }
            }
            // Fallback: check if phase is Running
            if let Some(PodPhase::Running) = status.phase {
                return true;
            }
        }
        false
    }

    /// Check if a pod is being deleted
    fn pod_is_deleting(pod: &Pod) -> bool {
        pod.metadata.deletion_timestamp.is_some()
    }
}

#[async_trait]
impl Controller for ReplicaSetController {
    fn name(&self) -> &str {
        "replicaset"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let rs_store = ResourceStore::<ReplicaSet>::new(self.storage.clone());
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        let replicasets = rs_store.list(None).await?;

        for rs in replicasets {
            let namespace = rs.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &rs.spec {
                Some(s) => s,
                None => continue,
            };

            let desired = spec.replicas.unwrap_or(1);

            // Find pods in this namespace
            let all_pods = pod_store.list(Some(namespace)).await?;

            // Filter to pods owned by this ReplicaSet
            let owned_pods: Vec<&Pod> = all_pods
                .iter()
                .filter(|p| Self::is_owned_by(p, &rs))
                .collect();

            // Count active (non-deleting) pods
            let active_pods: Vec<&Pod> = owned_pods
                .iter()
                .filter(|p| !Self::pod_is_deleting(p))
                .copied()
                .collect();

            let current = active_pods.len() as i32;

            debug!(
                "ReplicaSet {}/{}: desired={}, current={}",
                namespace, rs.metadata.name, desired, current
            );

            // Scale up: create pods
            if current < desired {
                let to_create = desired - current;
                info!(
                    "Scaling up ReplicaSet {}/{}: creating {} pods",
                    namespace, rs.metadata.name, to_create
                );

                for _ in 0..to_create {
                    let pod = Self::create_pod_from_template(&rs);
                    match pod_store.create(pod).await {
                        Ok(created) => {
                            info!(
                                "Created pod {}/{} for ReplicaSet {}",
                                namespace, created.metadata.name, rs.metadata.name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create pod for ReplicaSet {}/{}: {}",
                                namespace, rs.metadata.name, e
                            );
                        }
                    }
                }
            }

            // Scale down: delete excess pods
            if current > desired {
                let to_delete = (current - desired) as usize;
                info!(
                    "Scaling down ReplicaSet {}/{}: deleting {} pods",
                    namespace, rs.metadata.name, to_delete
                );

                // Delete pods that are not ready first, then oldest
                let mut pods_to_delete: Vec<&Pod> = active_pods.clone();
                pods_to_delete.sort_by(|a, b| {
                    let a_ready = Self::pod_is_ready(a);
                    let b_ready = Self::pod_is_ready(b);
                    // Delete non-ready pods first
                    match (a_ready, b_ready) {
                        (false, true) => std::cmp::Ordering::Less,
                        (true, false) => std::cmp::Ordering::Greater,
                        _ => {
                            // Then by creation timestamp (oldest first)
                            let a_time = a.metadata.creation_timestamp;
                            let b_time = b.metadata.creation_timestamp;
                            a_time.cmp(&b_time)
                        }
                    }
                });

                for pod in pods_to_delete.iter().take(to_delete) {
                    match pod_store.delete(Some(namespace), &pod.metadata.name).await {
                        Ok(_) => {
                            info!(
                                "Deleted pod {}/{} for ReplicaSet scale-down",
                                namespace, pod.metadata.name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to delete pod {}/{}: {}",
                                namespace, pod.metadata.name, e
                            );
                        }
                    }
                }
            }

            // Update ReplicaSet status
            let ready_count = active_pods
                .iter()
                .filter(|p| Self::pod_is_ready(p))
                .count() as i32;

            let current_status = rs.status.clone().unwrap_or_default();
            if current_status.replicas != current
                || current_status.ready_replicas != ready_count
                || current_status.available_replicas != ready_count
            {
                let mut updated_rs = rs.clone();
                updated_rs.status = Some(ReplicaSetStatus {
                    replicas: current,
                    ready_replicas: ready_count,
                    available_replicas: ready_count,
                    observed_generation: rs.metadata.generation,
                    conditions: current_status.conditions,
                });

                if let Err(e) = rs_store.update(updated_rs).await {
                    warn!(
                        "Failed to update ReplicaSet {}/{} status: {}",
                        namespace, rs.metadata.name, e
                    );
                }
            }
        }

        Ok(())
    }
}

/// Generate a random 5-character suffix for pod names
fn generate_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut suffix = String::new();
    let mut n = now as u64;
    for _ in 0..5 {
        suffix.push(chars[(n % 36) as usize]);
        n /= 36;
    }
    suffix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_suffix() {
        let s1 = generate_suffix();
        let s2 = generate_suffix();
        assert_eq!(s1.len(), 5);
        assert_eq!(s2.len(), 5);
        // They should be different (with high probability given nanosecond resolution)
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_labels_match_selector() {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());
        labels.insert("env".to_string(), "prod".to_string());

        let mut selector = BTreeMap::new();
        selector.insert("app".to_string(), "nginx".to_string());

        assert!(ReplicaSetController::labels_match_selector(&labels, &selector));

        selector.insert("app".to_string(), "redis".to_string());
        assert!(!ReplicaSetController::labels_match_selector(
            &labels, &selector
        ));
    }
}
