//! StatefulSet controller
//!
//! Manages pods with stable identities and persistent storage.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    apps_v1::{StatefulSet, StatefulSetStatus, PodManagementPolicyType},
    ObjectMeta, OwnerReference, PersistentVolumeClaim,
    Pod, PodPhase, TypeMeta,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct StatefulSetController {
    storage: Arc<SledBackend>,
}

impl StatefulSetController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a pod is owned by the given StatefulSet
    fn is_owned_by(pod: &Pod, sts: &StatefulSet) -> bool {
        pod.metadata.owner_references.iter().any(|owner| {
            owner.kind == "StatefulSet"
                && owner.name == sts.metadata.name
                && owner.uid == sts.metadata.uid
        })
    }

    /// Get the ordinal index from a pod name (e.g., "myapp-0" -> 0)
    fn pod_ordinal(sts_name: &str, pod_name: &str) -> Option<i32> {
        let prefix = format!("{sts_name}-");
        if pod_name.starts_with(&prefix) {
            pod_name[prefix.len()..].parse().ok()
        } else {
            None
        }
    }

    /// Create a pod for a specific ordinal
    fn create_pod_for_ordinal(sts: &StatefulSet, ordinal: i32) -> Pod {
        let spec = sts.spec.as_ref().expect("StatefulSet must have spec");
        let template = &spec.template;
        let namespace = sts
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // StatefulSet pods have predictable names
        let pod_name = format!("{}-{}", sts.metadata.name, ordinal);

        // Get labels from template
        let mut labels = template.metadata.labels.clone();
        for (k, v) in &spec.selector.match_labels {
            labels.insert(k.clone(), v.clone());
        }
        // Add statefulset-specific labels
        labels.insert(
            "statefulset.kubernetes.io/pod-name".to_string(),
            pod_name.clone(),
        );

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "apps/v1".to_string(),
            kind: "StatefulSet".to_string(),
            name: sts.metadata.name.clone(),
            uid: sts.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        // Set hostname for stable network identity
        let mut pod_spec = template.spec.clone().unwrap_or_default();
        pod_spec.hostname = Some(pod_name.clone());
        pod_spec.subdomain = Some(spec.service_name.clone());

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

    /// Create PVCs for a pod based on volume claim templates
    fn create_pvcs_for_pod(
        sts: &StatefulSet,
        ordinal: i32,
    ) -> Vec<PersistentVolumeClaim> {
        let spec = sts.spec.as_ref().expect("StatefulSet must have spec");
        let namespace = sts
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        spec.volume_claim_templates
            .iter()
            .map(|template| {
                let pvc_name = format!(
                    "{}-{}-{}",
                    template.metadata.name, sts.metadata.name, ordinal
                );

                let owner_ref = OwnerReference {
                    api_version: "apps/v1".to_string(),
                    kind: "StatefulSet".to_string(),
                    name: sts.metadata.name.clone(),
                    uid: sts.metadata.uid.clone(),
                    controller: Some(true),
                    block_owner_deletion: Some(false), // PVCs typically outlive StatefulSets
                };

                PersistentVolumeClaim {
                    api_version: "v1".to_string(),
                    kind: "PersistentVolumeClaim".to_string(),
                    metadata: ObjectMeta {
                        name: pvc_name,
                        namespace: Some(namespace.clone()),
                        labels: template.metadata.labels.clone(),
                        annotations: template.metadata.annotations.clone(),
                        owner_references: vec![owner_ref],
                        ..Default::default()
                    },
                    spec: Some(template.spec.clone()),
                    status: None,
                }
            })
            .collect()
    }

    /// Check if a pod is ready
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
}

#[async_trait]
impl Controller for StatefulSetController {
    fn name(&self) -> &str {
        "statefulset"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let sts_store = ResourceStore::<StatefulSet>::new(self.storage.clone());
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());
        let pvc_store = ResourceStore::<PersistentVolumeClaim>::new(self.storage.clone());

        let statefulsets = sts_store.list(None).await?;

        for sts in statefulsets {
            let namespace = sts.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &sts.spec {
                Some(s) => s,
                None => continue,
            };

            let desired_replicas = spec.replicas.unwrap_or(1);
            let policy = spec
                .pod_management_policy
                .as_ref()
                .unwrap_or(&PodManagementPolicyType::OrderedReady);

            // Find pods owned by this StatefulSet
            let all_pods = pod_store.list(Some(namespace)).await?;
            let owned_pods: Vec<&Pod> = all_pods
                .iter()
                .filter(|p| Self::is_owned_by(p, &sts))
                .collect();

            // Build ordinal -> pod mapping
            let mut ordinal_to_pod: BTreeMap<i32, &Pod> = BTreeMap::new();
            for pod in &owned_pods {
                if let Some(ordinal) = Self::pod_ordinal(&sts.metadata.name, &pod.metadata.name) {
                    if !Self::pod_is_deleting(pod) {
                        ordinal_to_pod.insert(ordinal, pod);
                    }
                }
            }

            let current_replicas = ordinal_to_pod.len() as i32;

            debug!(
                "StatefulSet {}/{}: desired={}, current={}",
                namespace, sts.metadata.name, desired_replicas, current_replicas
            );

            // Scale up: create pods in order
            if current_replicas < desired_replicas {
                let start_ordinal = spec.ordinals.as_ref().map(|o| o.start).unwrap_or(0);

                for ordinal in start_ordinal..(start_ordinal + desired_replicas) {
                    if ordinal_to_pod.contains_key(&ordinal) {
                        // Pod exists, check if it's ready for OrderedReady policy
                        if *policy == PodManagementPolicyType::OrderedReady {
                            if let Some(pod) = ordinal_to_pod.get(&ordinal) {
                                if !Self::pod_is_ready(pod) {
                                    debug!(
                                        "Waiting for pod {}-{} to be ready before creating next",
                                        sts.metadata.name, ordinal
                                    );
                                    break;
                                }
                            }
                        }
                        continue;
                    }

                    // For OrderedReady, ensure previous pod is ready
                    if *policy == PodManagementPolicyType::OrderedReady && ordinal > start_ordinal {
                        let prev_ordinal = ordinal - 1;
                        if let Some(prev_pod) = ordinal_to_pod.get(&prev_ordinal) {
                            if !Self::pod_is_ready(prev_pod) {
                                debug!(
                                    "Waiting for pod {}-{} to be ready",
                                    sts.metadata.name, prev_ordinal
                                );
                                break;
                            }
                        } else {
                            // Previous pod doesn't exist, can't create this one
                            break;
                        }
                    }

                    info!(
                        "Creating pod {}-{} for StatefulSet {}/{}",
                        sts.metadata.name, ordinal, namespace, sts.metadata.name
                    );

                    // Create PVCs first
                    let pvcs = Self::create_pvcs_for_pod(&sts, ordinal);
                    for pvc in pvcs {
                        // Check if PVC already exists
                        if pvc_store.get(Some(namespace), &pvc.metadata.name).await?.is_none() {
                            match pvc_store.create(pvc).await {
                                Ok(created) => {
                                    info!(
                                        "Created PVC {} for StatefulSet {}/{}",
                                        created.metadata.name, namespace, sts.metadata.name
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to create PVC for StatefulSet {}/{}: {}",
                                        namespace, sts.metadata.name, e
                                    );
                                }
                            }
                        }
                    }

                    // Create the pod
                    let pod = Self::create_pod_for_ordinal(&sts, ordinal);
                    match pod_store.create(pod).await {
                        Ok(created) => {
                            info!(
                                "Created pod {} for StatefulSet {}/{}",
                                created.metadata.name, namespace, sts.metadata.name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create pod for StatefulSet {}/{}: {}",
                                namespace, sts.metadata.name, e
                            );
                            break;
                        }
                    }

                    // For OrderedReady, only create one at a time
                    if *policy == PodManagementPolicyType::OrderedReady {
                        break;
                    }
                }
            }

            // Scale down: delete pods in reverse order
            if current_replicas > desired_replicas {
                let start_ordinal = spec.ordinals.as_ref().map(|o| o.start).unwrap_or(0);
                let max_ordinal = start_ordinal + current_replicas - 1;

                for ordinal in (start_ordinal + desired_replicas..=max_ordinal).rev() {
                    if let Some(pod) = ordinal_to_pod.get(&ordinal) {
                        info!(
                            "Deleting pod {} for StatefulSet scale-down",
                            pod.metadata.name
                        );

                        if let Err(e) = pod_store.delete(Some(namespace), &pod.metadata.name).await {
                            warn!(
                                "Failed to delete pod {}: {}",
                                pod.metadata.name, e
                            );
                        }

                        // For OrderedReady, only delete one at a time
                        if *policy == PodManagementPolicyType::OrderedReady {
                            break;
                        }
                    }
                }
            }

            // Update status
            let ready_count = ordinal_to_pod
                .values()
                .filter(|p| Self::pod_is_ready(p))
                .count() as i32;

            let current_status = sts.status.clone().unwrap_or_default();
            if current_status.replicas != current_replicas
                || current_status.ready_replicas != ready_count
            {
                let mut updated_sts = sts.clone();
                updated_sts.status = Some(StatefulSetStatus {
                    replicas: current_replicas,
                    ready_replicas: ready_count,
                    current_replicas,
                    updated_replicas: current_replicas,
                    available_replicas: ready_count,
                    observed_generation: sts.metadata.generation,
                    current_revision: current_status.current_revision,
                    update_revision: current_status.update_revision,
                    collision_count: current_status.collision_count,
                    conditions: current_status.conditions,
                });

                if let Err(e) = sts_store.update(updated_sts).await {
                    warn!(
                        "Failed to update StatefulSet {}/{} status: {}",
                        namespace, sts.metadata.name, e
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

    #[test]
    fn test_pod_ordinal() {
        assert_eq!(StatefulSetController::pod_ordinal("web", "web-0"), Some(0));
        assert_eq!(StatefulSetController::pod_ordinal("web", "web-5"), Some(5));
        assert_eq!(StatefulSetController::pod_ordinal("web", "web-123"), Some(123));
        assert_eq!(StatefulSetController::pod_ordinal("web", "other-0"), None);
        assert_eq!(StatefulSetController::pod_ordinal("web", "web-abc"), None);
    }
}
