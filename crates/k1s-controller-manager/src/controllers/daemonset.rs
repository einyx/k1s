//! DaemonSet controller
//!
//! Ensures that a pod runs on every node (or a subset based on node selectors).

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    apps_v1::{DaemonSet, DaemonSetStatus},
    Node, NodeConditionType, ObjectMeta, OwnerReference, Pod, PodPhase, TypeMeta,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct DaemonSetController {
    storage: Arc<SledBackend>,
}

impl DaemonSetController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a pod is owned by the given DaemonSet
    fn is_owned_by(pod: &Pod, ds: &DaemonSet) -> bool {
        pod.metadata.owner_references.iter().any(|owner| {
            owner.kind == "DaemonSet"
                && owner.name == ds.metadata.name
                && owner.uid == ds.metadata.uid
        })
    }

    /// Check if labels match the selector
    fn labels_match_selector(
        labels: &BTreeMap<String, String>,
        selector: &BTreeMap<String, String>,
    ) -> bool {
        selector.iter().all(|(k, v)| labels.get(k) == Some(v))
    }

    /// Check if node is schedulable (not cordoned)
    fn node_is_schedulable(node: &Node) -> bool {
        // Check if node is unschedulable
        if node
            .spec
            .as_ref()
            .map(|s| s.unschedulable)
            .unwrap_or(false)
        {
            return false;
        }

        // Check for Ready condition
        if let Some(status) = &node.status {
            for condition in &status.conditions {
                if condition.r#type == NodeConditionType::Ready && condition.status == "True" {
                    return true;
                }
            }
            // No Ready=True condition found
            return false;
        }

        // No status, assume schedulable
        true
    }

    /// Check if pod should run on this node based on node selector
    fn should_schedule_on_node(
        ds: &DaemonSet,
        node: &Node,
    ) -> bool {
        let spec = match &ds.spec {
            Some(s) => s,
            None => return false,
        };

        // Check node selector from pod template
        if let Some(pod_spec) = &spec.template.spec {
            if !pod_spec.node_selector.is_empty() {
                return Self::labels_match_selector(&node.metadata.labels, &pod_spec.node_selector);
            }
        }

        // No node selector means schedule on all nodes
        true
    }

    /// Create a pod from DaemonSet template for a specific node
    fn create_pod_for_node(ds: &DaemonSet, node: &Node) -> Pod {
        let spec = ds.spec.as_ref().expect("DaemonSet must have spec");
        let template = &spec.template;
        let namespace = ds
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // Generate unique pod name
        let pod_name = format!("{}-{}", ds.metadata.name, generate_suffix());

        // Get labels from template, ensure selector labels are present
        let mut labels = template.metadata.labels.clone();
        for (k, v) in &spec.selector.match_labels {
            labels.insert(k.clone(), v.clone());
        }
        // Add controller-revision-hash for tracking
        labels.insert(
            "controller-revision-hash".to_string(),
            generate_revision_hash(ds),
        );

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "apps/v1".to_string(),
            kind: "DaemonSet".to_string(),
            name: ds.metadata.name.clone(),
            uid: ds.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        // Copy pod spec and set node name for scheduling
        let mut pod_spec = template.spec.clone().unwrap_or_default();
        pod_spec.node_name = Some(node.metadata.name.clone());

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
            spec: Some(pod_spec),
            status: None,
        }
    }

    /// Check if a pod is considered ready
    fn pod_is_ready(pod: &Pod) -> bool {
        if let Some(status) = &pod.status {
            for condition in &status.conditions {
                if condition.r#type == "Ready" && condition.status == "True" {
                    return true;
                }
            }
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

    /// Get the node name for a pod
    fn pod_node_name(pod: &Pod) -> Option<&str> {
        pod.spec.as_ref()?.node_name.as_deref()
    }
}

#[async_trait]
impl Controller for DaemonSetController {
    fn name(&self) -> &str {
        "daemonset"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let ds_store = ResourceStore::<DaemonSet>::new(self.storage.clone());
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());
        let node_store = ResourceStore::<Node>::new(self.storage.clone());

        let daemonsets = ds_store.list(None).await?;
        let all_nodes = node_store.list(None).await?;

        for ds in daemonsets {
            let namespace = ds.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &ds.spec {
                Some(s) => s,
                None => continue,
            };

            // Find pods owned by this DaemonSet
            let all_pods = pod_store.list(Some(namespace)).await?;
            let owned_pods: Vec<&Pod> = all_pods.iter().filter(|p| Self::is_owned_by(p, &ds)).collect();

            // Build a map of node -> pod for this DaemonSet
            let mut node_to_pod: BTreeMap<String, Vec<&Pod>> = BTreeMap::new();
            for pod in &owned_pods {
                if let Some(node_name) = Self::pod_node_name(pod) {
                    node_to_pod
                        .entry(node_name.to_string())
                        .or_default()
                        .push(pod);
                }
            }

            // Track status counters
            let mut desired_number_scheduled = 0;
            let mut current_number_scheduled = 0;
            let mut number_ready = 0;
            let mut number_available = 0;
            let mut number_misscheduled = 0;

            // Process each node
            for node in &all_nodes {
                let should_schedule = Self::should_schedule_on_node(&ds, node);
                let is_schedulable = Self::node_is_schedulable(node);
                let node_name = &node.metadata.name;

                // Get pods on this node
                let pods_on_node = node_to_pod.get(node_name).cloned().unwrap_or_default();
                let active_pods: Vec<&&Pod> = pods_on_node
                    .iter()
                    .filter(|p| !Self::pod_is_deleting(p))
                    .collect();

                if should_schedule && is_schedulable {
                    desired_number_scheduled += 1;

                    if active_pods.is_empty() {
                        // Need to create a pod on this node
                        info!(
                            "Creating DaemonSet pod for {}/{} on node {}",
                            namespace, ds.metadata.name, node_name
                        );
                        let pod = Self::create_pod_for_node(&ds, node);
                        match pod_store.create(pod).await {
                            Ok(created) => {
                                info!(
                                    "Created pod {}/{} for DaemonSet {} on node {}",
                                    namespace, created.metadata.name, ds.metadata.name, node_name
                                );
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to create pod for DaemonSet {}/{} on node {}: {}",
                                    namespace, ds.metadata.name, node_name, e
                                );
                            }
                        }
                    } else {
                        // Pod exists on this node
                        current_number_scheduled += 1;

                        // Count ready/available
                        for pod in &active_pods {
                            if Self::pod_is_ready(pod) {
                                number_ready += 1;
                                number_available += 1;
                            }
                        }

                        // If multiple pods exist, delete extras
                        if active_pods.len() > 1 {
                            warn!(
                                "Multiple pods for DaemonSet {}/{} on node {}, cleaning up",
                                namespace, ds.metadata.name, node_name
                            );
                            for extra_pod in active_pods.iter().skip(1) {
                                if let Err(e) = pod_store
                                    .delete(Some(namespace), &extra_pod.metadata.name)
                                    .await
                                {
                                    warn!(
                                        "Failed to delete extra pod {}/{}: {}",
                                        namespace, extra_pod.metadata.name, e
                                    );
                                }
                            }
                        }
                    }
                } else if !active_pods.is_empty() {
                    // Pods exist but shouldn't be scheduled here
                    number_misscheduled += active_pods.len() as i32;
                    info!(
                        "Deleting misscheduled DaemonSet pods for {}/{} on node {}",
                        namespace, ds.metadata.name, node_name
                    );
                    for pod in active_pods {
                        if let Err(e) =
                            pod_store.delete(Some(namespace), &pod.metadata.name).await
                        {
                            warn!(
                                "Failed to delete misscheduled pod {}/{}: {}",
                                namespace, pod.metadata.name, e
                            );
                        }
                    }
                }
            }

            debug!(
                "DaemonSet {}/{}: desired={}, current={}, ready={}, available={}, misscheduled={}",
                namespace,
                ds.metadata.name,
                desired_number_scheduled,
                current_number_scheduled,
                number_ready,
                number_available,
                number_misscheduled
            );

            // Update DaemonSet status
            let number_unavailable = desired_number_scheduled - number_available;
            let current_status = ds.status.clone().unwrap_or_default();

            if current_status.desired_number_scheduled != desired_number_scheduled
                || current_status.current_number_scheduled != current_number_scheduled
                || current_status.number_ready != number_ready
                || current_status.number_available != number_available
                || current_status.number_unavailable != number_unavailable
                || current_status.number_misscheduled != number_misscheduled
            {
                let mut updated_ds = ds.clone();
                updated_ds.status = Some(DaemonSetStatus {
                    desired_number_scheduled,
                    current_number_scheduled,
                    number_ready,
                    number_available,
                    number_unavailable,
                    number_misscheduled,
                    updated_number_scheduled: current_number_scheduled,
                    observed_generation: ds.metadata.generation,
                    conditions: current_status.conditions,
                    collision_count: current_status.collision_count,
                });

                if let Err(e) = ds_store.update(updated_ds).await {
                    warn!(
                        "Failed to update DaemonSet {}/{} status: {}",
                        namespace, ds.metadata.name, e
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

/// Generate a simple revision hash for the DaemonSet
fn generate_revision_hash(ds: &DaemonSet) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    ds.metadata.generation.hash(&mut hasher);
    format!("{:x}", hasher.finish())[..8].to_string()
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
    }

    #[test]
    fn test_labels_match_selector() {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());
        labels.insert("env".to_string(), "prod".to_string());

        let mut selector = BTreeMap::new();
        selector.insert("app".to_string(), "nginx".to_string());

        assert!(DaemonSetController::labels_match_selector(&labels, &selector));

        selector.insert("app".to_string(), "redis".to_string());
        assert!(!DaemonSetController::labels_match_selector(&labels, &selector));
    }
}
