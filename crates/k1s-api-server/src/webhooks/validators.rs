//! Built-in validation webhooks

use k1s_types::api::admission::v1::{AdmissionRequest, AdmissionResponse};
use k1s_types::{Pod, Deployment, StatefulSet, DaemonSet};
use serde_json::Value;
use tracing::debug;

/// Validates Pod resources
pub fn validate_pod(request: &AdmissionRequest) -> AdmissionResponse {
    let pod: Pod = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(p) => p,
        None => return AdmissionResponse::deny(request.uid.clone(), "Invalid pod object".to_string()),
    };

    // Validate pod name length (DNS-1123 subdomain)
    if pod.metadata.name.len() > 253 {
        return AdmissionResponse::deny(
            request.uid.clone(),
            format!("Pod name too long: {} characters (max 253)", pod.metadata.name.len()),
        );
    }

    // Validate pod name format (lowercase alphanumeric and dashes)
    if !pod.metadata.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "Pod name must contain only lowercase alphanumeric characters or '-'".to_string(),
        );
    }

    // Validate pod spec exists
    let spec = match &pod.spec {
        Some(s) => s,
        None => return AdmissionResponse::deny(request.uid.clone(), "Pod spec is required".to_string()),
    };

    // Validate at least one container
    if spec.containers.is_empty() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "Pod must have at least one container".to_string(),
        );
    }

    // Validate container names are unique
    let mut container_names = std::collections::HashSet::new();
    for container in &spec.containers {
        if !container_names.insert(&container.name) {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Duplicate container name: {}", container.name),
            );
        }

        // Validate container name format
        if container.name.is_empty() || container.name.len() > 63 {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Container name must be 1-63 characters: {}", container.name),
            );
        }

        // Validate image is specified
        if container.image.is_empty() {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Container {} must specify an image", container.name),
            );
        }
    }

    // Validate init containers
    for init_container in &spec.init_containers {
        if init_container.image.is_empty() {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Init container {} must specify an image", init_container.name),
            );
        }
    }

    // Add warnings for deprecated or risky configurations
    let mut response = AdmissionResponse::allow(request.uid.clone());

    // Warn about latest tag
    for container in &spec.containers {
        if container.image.ends_with(":latest") || !container.image.contains(':') {
            response = response.with_warning(format!(
                "Container {} uses latest tag or no tag, which is not recommended for production",
                container.name
            ));
        }
    }

    // Warn about privileged containers
    for container in &spec.containers {
        if let Some(security_context) = &container.security_context {
            if security_context.privileged.unwrap_or(false) {
                response = response.with_warning(format!(
                    "Container {} runs in privileged mode, which is a security risk",
                    container.name
                ));
            }
        }
    }

    // Warn about host network
    if spec.host_network {
        response = response.with_warning("Pod uses host network, which may be a security risk".to_string());
    }

    debug!("Pod {} validated successfully", pod.metadata.name);
    response
}

/// Validates Deployment resources
pub fn validate_deployment(request: &AdmissionRequest) -> AdmissionResponse {
    let deployment: Deployment = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(d) => d,
        None => return AdmissionResponse::deny(request.uid.clone(), "Invalid deployment object".to_string()),
    };

    // Validate deployment spec exists
    let spec = match &deployment.spec {
        Some(s) => s,
        None => return AdmissionResponse::deny(request.uid.clone(), "Deployment spec is required".to_string()),
    };

    // Validate replicas is not negative
    if let Some(replicas) = spec.replicas {
        if replicas < 0 {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Replicas cannot be negative: {}", replicas),
            );
        }
    }

    // Validate selector
    if spec.selector.match_labels.is_empty() && spec.selector.match_expressions.is_empty() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "Deployment selector must have at least one match label or expression".to_string(),
        );
    }

    // Validate template labels match selector
    for (key, value) in &spec.selector.match_labels {
        if spec.template.metadata.labels.get(key) != Some(value) {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Selector label {}={} not found in pod template labels", key, value),
            );
        }
    }

    // Validate pod template
    if spec.template.spec.is_none() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "Deployment template must have a pod spec".to_string(),
        );
    }

    let mut response = AdmissionResponse::allow(request.uid.clone());

    // Warn about high replica count
    if let Some(replicas) = spec.replicas {
        if replicas > 100 {
            response = response.with_warning(format!(
                "High replica count ({}), ensure cluster has sufficient resources",
                replicas
            ));
        }
    }

    debug!("Deployment {} validated successfully", deployment.metadata.name);
    response
}

/// Validates StatefulSet resources
pub fn validate_statefulset(request: &AdmissionRequest) -> AdmissionResponse {
    let sts: StatefulSet = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(s) => s,
        None => return AdmissionResponse::deny(request.uid.clone(), "Invalid statefulset object".to_string()),
    };

    let spec = match &sts.spec {
        Some(s) => s,
        None => return AdmissionResponse::deny(request.uid.clone(), "StatefulSet spec is required".to_string()),
    };

    // Validate service name
    if spec.service_name.is_empty() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "StatefulSet must specify a serviceName".to_string(),
        );
    }

    // Validate selector
    if spec.selector.match_labels.is_empty() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "StatefulSet selector must have at least one match label".to_string(),
        );
    }

    // Validate template labels match selector
    for (key, value) in &spec.selector.match_labels {
        if spec.template.metadata.labels.get(key) != Some(value) {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Selector label {}={} not found in pod template labels", key, value),
            );
        }
    }

    debug!("StatefulSet {} validated successfully", sts.metadata.name);
    AdmissionResponse::allow(request.uid.clone())
}

/// Validates DaemonSet resources
pub fn validate_daemonset(request: &AdmissionRequest) -> AdmissionResponse {
    let ds: DaemonSet = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(d) => d,
        None => return AdmissionResponse::deny(request.uid.clone(), "Invalid daemonset object".to_string()),
    };

    let spec = match &ds.spec {
        Some(s) => s,
        None => return AdmissionResponse::deny(request.uid.clone(), "DaemonSet spec is required".to_string()),
    };

    // Validate selector
    if spec.selector.match_labels.is_empty() {
        return AdmissionResponse::deny(
            request.uid.clone(),
            "DaemonSet selector must have at least one match label".to_string(),
        );
    }

    // Validate template labels match selector
    for (key, value) in &spec.selector.match_labels {
        if spec.template.metadata.labels.get(key) != Some(value) {
            return AdmissionResponse::deny(
                request.uid.clone(),
                format!("Selector label {}={} not found in pod template labels", key, value),
            );
        }
    }

    debug!("DaemonSet {} validated successfully", ds.metadata.name);
    AdmissionResponse::allow(request.uid.clone())
}

/// Route validation request to appropriate validator based on resource kind
pub fn validate_resource(request: &AdmissionRequest) -> AdmissionResponse {
    match request.kind.kind.as_str() {
        "Pod" => validate_pod(request),
        "Deployment" => validate_deployment(request),
        "StatefulSet" => validate_statefulset(request),
        "DaemonSet" => validate_daemonset(request),
        _ => {
            // Unknown resource type, allow by default
            debug!("No built-in validator for kind: {}", request.kind.kind);
            AdmissionResponse::allow(request.uid.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1s_types::api::admission::v1::{GroupVersionKind, GroupVersionResource, Operation, UserInfo};
    use k1s_types::{PodSpec, Container, ObjectMeta};

    fn make_request(kind: &str, object: Value) -> AdmissionRequest {
        AdmissionRequest {
            uid: "test-uid".to_string(),
            kind: GroupVersionKind {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: kind.to_string(),
            },
            resource: GroupVersionResource {
                group: "".to_string(),
                version: "v1".to_string(),
                resource: kind.to_lowercase() + "s",
            },
            sub_resource: None,
            request_kind: None,
            request_resource: None,
            name: Some("test".to_string()),
            namespace: Some("default".to_string()),
            operation: Operation::Create,
            user_info: UserInfo {
                username: "test-user".to_string(),
                uid: None,
                groups: vec![],
                extra: None,
            },
            object: Some(object),
            old_object: None,
            dry_run: None,
            options: None,
        }
    }

    #[test]
    fn test_validate_pod_success() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: "test-pod".to_string(),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "nginx".to_string(),
                    image: "nginx:1.21".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let request = make_request("Pod", serde_json::to_value(&pod).unwrap());
        let response = validate_pod(&request);
        assert!(response.allowed);
    }

    #[test]
    fn test_validate_pod_no_containers() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: "test-pod".to_string(),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };

        let request = make_request("Pod", serde_json::to_value(&pod).unwrap());
        let response = validate_pod(&request);
        assert!(!response.allowed);
        assert!(response.status.is_some());
    }

    #[test]
    fn test_validate_pod_latest_tag_warning() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: "test-pod".to_string(),
                ..Default::default()
            },
            spec: Some(PodSpec {
                containers: vec![Container {
                    name: "nginx".to_string(),
                    image: "nginx:latest".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };

        let request = make_request("Pod", serde_json::to_value(&pod).unwrap());
        let response = validate_pod(&request);
        assert!(response.allowed);
        assert!(response.warnings.is_some());
    }
}
