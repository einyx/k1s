//! Endpoints controller
//!
//! Creates and updates Endpoints objects based on Services and their selected Pods.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    EndpointAddress, EndpointPort, EndpointSubset, Endpoints, ObjectMeta, ObjectReference, Pod,
    PodPhase, Protocol, Service, ServicePort, TypeMeta,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct EndpointsController {
    storage: Arc<SledBackend>,
}

impl EndpointsController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if pod labels match the service selector
    fn labels_match_selector(
        labels: &BTreeMap<String, String>,
        selector: &BTreeMap<String, String>,
    ) -> bool {
        if selector.is_empty() {
            return false; // Services without selectors don't select pods
        }
        selector.iter().all(|(k, v)| labels.get(k) == Some(v))
    }

    /// Check if a pod is ready to receive traffic
    fn pod_is_ready(pod: &Pod) -> bool {
        // Pod must have an IP
        let has_ip = pod
            .status
            .as_ref()
            .and_then(|s| s.pod_ip.as_ref())
            .map(|ip| !ip.is_empty())
            .unwrap_or(false);

        if !has_ip {
            return false;
        }

        // Check if pod is running
        let is_running = pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| matches!(p, PodPhase::Running))
            .unwrap_or(false);

        if !is_running {
            return false;
        }

        // Check for Ready condition
        if let Some(status) = &pod.status {
            for condition in &status.conditions {
                if condition.r#type == "Ready" {
                    return condition.status == "True";
                }
            }
        }

        // If no Ready condition, consider running pods as ready
        true
    }

    /// Convert service ports to endpoint ports
    fn service_ports_to_endpoint_ports(service_ports: &[ServicePort]) -> Vec<EndpointPort> {
        service_ports
            .iter()
            .map(|sp| EndpointPort {
                name: sp.name.clone(),
                port: sp.target_port
                    .as_ref()
                    .map(|tp| match tp {
                        k1s_types::IntOrString::Int(i) => *i,
                        k1s_types::IntOrString::String(_) => sp.port, // Named ports not fully supported yet
                    })
                    .unwrap_or(sp.port),
                protocol: sp.protocol.clone(),
                app_protocol: sp.app_protocol.clone(),
            })
            .collect()
    }

    /// Create an Endpoints object for a Service
    fn create_endpoints_for_service(
        service: &Service,
        ready_pods: &[&Pod],
        not_ready_pods: &[&Pod],
    ) -> Endpoints {
        let namespace = service
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        let spec = service.spec.as_ref();
        let service_ports = spec.map(|s| &s.ports).map(|p| p.as_slice()).unwrap_or(&[]);

        // Build addresses from ready pods
        let ready_addresses: Vec<EndpointAddress> = ready_pods
            .iter()
            .filter_map(|pod| {
                let ip = pod.status.as_ref()?.pod_ip.as_ref()?.clone();
                let node_name = pod.spec.as_ref()?.node_name.clone();

                Some(
                    EndpointAddress::new(&ip)
                        .with_target_ref(ObjectReference::from_pod(
                            &pod.metadata.name,
                            &namespace,
                            &pod.metadata.uid,
                            &pod.metadata.resource_version,
                        ))
                        .with_node_name(node_name.as_deref().unwrap_or("")),
                )
            })
            .collect();

        // Build addresses from not-ready pods
        let not_ready_addresses: Vec<EndpointAddress> = not_ready_pods
            .iter()
            .filter_map(|pod| {
                let ip = pod.status.as_ref()?.pod_ip.as_ref()?.clone();
                let node_name = pod.spec.as_ref()?.node_name.clone();

                Some(
                    EndpointAddress::new(&ip)
                        .with_target_ref(ObjectReference::from_pod(
                            &pod.metadata.name,
                            &namespace,
                            &pod.metadata.uid,
                            &pod.metadata.resource_version,
                        ))
                        .with_node_name(node_name.as_deref().unwrap_or("")),
                )
            })
            .collect();

        // Convert service ports to endpoint ports
        let endpoint_ports = Self::service_ports_to_endpoint_ports(service_ports);

        // Create subset (only if we have addresses or ports)
        let mut subsets = Vec::new();
        if !ready_addresses.is_empty() || !not_ready_addresses.is_empty() || !endpoint_ports.is_empty()
        {
            subsets.push(
                EndpointSubset::new()
                    .with_addresses(ready_addresses)
                    .with_not_ready_addresses(not_ready_addresses)
                    .with_ports(endpoint_ports),
            );
        }

        // Endpoints object has the same name as the Service
        Endpoints {
            type_meta: TypeMeta::new("v1", "Endpoints"),
            metadata: ObjectMeta {
                name: service.metadata.name.clone(),
                namespace: Some(namespace),
                labels: service.metadata.labels.clone(),
                ..Default::default()
            },
            subsets,
        }
    }
}

#[async_trait]
impl Controller for EndpointsController {
    fn name(&self) -> &str {
        "endpoints"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let service_store = ResourceStore::<Service>::new(self.storage.clone());
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());
        let endpoints_store = ResourceStore::<Endpoints>::new(self.storage.clone());

        let services = service_store.list(None).await?;

        for service in services {
            let namespace = service.metadata.namespace.as_deref().unwrap_or("default");

            // Get the service selector
            let selector = service
                .spec
                .as_ref()
                .map(|s| &s.selector)
                .cloned()
                .unwrap_or_default();

            // Skip services without selectors (e.g., ExternalName services)
            if selector.is_empty() {
                debug!(
                    "Service {}/{} has no selector, skipping endpoints creation",
                    namespace, service.metadata.name
                );
                continue;
            }

            // Find pods matching the selector in the same namespace
            let all_pods = pod_store.list(Some(namespace)).await?;
            let matching_pods: Vec<&Pod> = all_pods
                .iter()
                .filter(|pod| {
                    // Skip pods being deleted
                    if pod.metadata.deletion_timestamp.is_some() {
                        return false;
                    }
                    Self::labels_match_selector(&pod.metadata.labels, &selector)
                })
                .collect();

            // Separate ready and not-ready pods
            let ready_pods: Vec<&Pod> = matching_pods
                .iter()
                .filter(|p| Self::pod_is_ready(p))
                .copied()
                .collect();
            let not_ready_pods: Vec<&Pod> = matching_pods
                .iter()
                .filter(|p| !Self::pod_is_ready(p))
                .copied()
                .collect();

            debug!(
                "Service {}/{}: found {} ready pods, {} not-ready pods",
                namespace,
                service.metadata.name,
                ready_pods.len(),
                not_ready_pods.len()
            );

            // Create the endpoints object
            let endpoints = Self::create_endpoints_for_service(&service, &ready_pods, &not_ready_pods);

            // Check if endpoints already exist
            let existing = endpoints_store
                .get(Some(namespace), &service.metadata.name)
                .await?;

            match existing {
                Some(existing_ep) => {
                    // Update if subsets changed
                    if existing_ep.subsets != endpoints.subsets {
                        let mut updated = existing_ep.clone();
                        updated.subsets = endpoints.subsets;
                        match endpoints_store.update(updated).await {
                            Ok(_) => {
                                info!(
                                    "Updated Endpoints {}/{} with {} ready addresses",
                                    namespace,
                                    service.metadata.name,
                                    ready_pods.len()
                                );
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to update Endpoints {}/{}: {}",
                                    namespace, service.metadata.name, e
                                );
                            }
                        }
                    }
                }
                None => {
                    // Create new endpoints
                    match endpoints_store.create(endpoints).await {
                        Ok(_) => {
                            info!(
                                "Created Endpoints {}/{} with {} ready addresses",
                                namespace,
                                service.metadata.name,
                                ready_pods.len()
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create Endpoints {}/{}: {}",
                                namespace, service.metadata.name, e
                            );
                        }
                    }
                }
            }
        }

        // Clean up orphaned endpoints (endpoints without matching service)
        let all_endpoints = endpoints_store.list(None).await?;
        for ep in all_endpoints {
            let namespace = ep.metadata.namespace.as_deref().unwrap_or("default");
            let service_exists = service_store
                .get(Some(namespace), &ep.metadata.name)
                .await?
                .is_some();

            if !service_exists {
                info!(
                    "Deleting orphaned Endpoints {}/{}",
                    namespace, ep.metadata.name
                );
                let _ = endpoints_store.delete(Some(namespace), &ep.metadata.name).await;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labels_match_selector() {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());
        labels.insert("env".to_string(), "prod".to_string());

        let mut selector = BTreeMap::new();
        selector.insert("app".to_string(), "nginx".to_string());

        assert!(EndpointsController::labels_match_selector(&labels, &selector));

        selector.insert("env".to_string(), "staging".to_string());
        assert!(!EndpointsController::labels_match_selector(
            &labels, &selector
        ));
    }

    #[test]
    fn test_empty_selector_returns_false() {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), "nginx".to_string());

        let selector = BTreeMap::new();
        assert!(!EndpointsController::labels_match_selector(&labels, &selector));
    }
}
