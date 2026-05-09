//! Built-in mutation webhooks

use k1s_types::api::admission::v1::{AdmissionRequest, AdmissionResponse};
use k1s_types::{Pod, Deployment, DaemonSet, StatefulSet, ReplicaSet, CronJob};
use serde_json::{json, Value};
use tracing::debug;

/// Mutates Pod resources with default values and best practices
pub fn mutate_pod(request: &AdmissionRequest) -> AdmissionResponse {
    let mut pod: Pod = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(p) => p,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Ensure labels object exists
    let labels_exist = !pod.metadata.labels.is_empty();
    if !labels_exist && request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("labels"))
        .is_none()
    {
        patches.push(json!({
            "op": "add",
            "path": "/metadata/labels",
            "value": {}
        }));
        modified = true;
    }

    // Add default labels if not present
    if !pod.metadata.labels.contains_key("app") {
        if let Some(name) = extract_app_name(&pod.metadata.name) {
            pod.metadata.labels.insert("app".to_string(), name.clone());
            patches.push(json!({
                "op": "add",
                "path": "/metadata/labels/app",
                "value": name
            }));
            modified = true;
        }
    }

    // Ensure annotations object exists
    let annotations_exist = !pod.metadata.annotations.is_empty();
    if !annotations_exist && request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("annotations"))
        .is_none()
    {
        patches.push(json!({
            "op": "add",
            "path": "/metadata/annotations",
            "value": {}
        }));
        modified = true;
    }

    // Add default annotations
    if pod.metadata.annotations.get("k1s.io/managed-by").is_none() {
        pod.metadata.annotations.insert(
            "k1s.io/managed-by".to_string(),
            "k1s".to_string(),
        );
        patches.push(json!({
            "op": "add",
            "path": "/metadata/annotations/k1s.io~1managed-by",
            "value": "k1s"
        }));
        modified = true;
    }

    // Add resource limits for containers without them
    if let Some(spec) = &mut pod.spec {
        for (idx, container) in spec.containers.iter_mut().enumerate() {
            // Check if resources field exists in original request
            let resources_exist = request.object.as_ref()
                .and_then(|o| o.get("spec"))
                .and_then(|s| s.get("containers"))
                .and_then(|c| c.get(idx))
                .and_then(|cont| cont.get("resources"))
                .is_some();

            if !resources_exist {
                // Create resources object first
                patches.push(json!({
                    "op": "add",
                    "path": format!("/spec/containers/{}/resources", idx),
                    "value": {}
                }));
                modified = true;
            }

            // Get or create resources
            let resources = container.resources.get_or_insert_with(Default::default);

            if resources.requests.is_empty() || resources.limits.is_empty() {
                // Add default resource requests/limits
                if resources.requests.is_empty() {
                    resources.requests.insert("cpu".to_string(), "100m".to_string());
                    resources.requests.insert("memory".to_string(), "128Mi".to_string());
                    patches.push(json!({
                        "op": "add",
                        "path": format!("/spec/containers/{}/resources/requests", idx),
                        "value": {
                            "cpu": "100m",
                            "memory": "128Mi"
                        }
                    }));
                    modified = true;
                }
                if resources.limits.is_empty() {
                    resources.limits.insert("cpu".to_string(), "500m".to_string());
                    resources.limits.insert("memory".to_string(), "512Mi".to_string());
                    patches.push(json!({
                        "op": "add",
                        "path": format!("/spec/containers/{}/resources/limits", idx),
                        "value": {
                            "cpu": "500m",
                            "memory": "512Mi"
                        }
                    }));
                    modified = true;
                }
            }
        }

        // Set default restart policy if not specified
        if spec.restart_policy.is_none() {
            patches.push(json!({
                "op": "add",
                "path": "/spec/restartPolicy",
                "value": "Always"
            }));
            modified = true;
        }
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        let patch_base64 = base64::encode(&patch_json);
        debug!("Mutated pod {} with {} patches", pod.metadata.name, patches.len());
        AdmissionResponse::mutate(request.uid.clone(), patch_base64)
            .with_audit_annotation("k1s.io/mutations".to_string(), format!("{} changes", patches.len()))
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

/// Mutates Deployment resources with defaults
pub fn mutate_deployment(request: &AdmissionRequest) -> AdmissionResponse {
    let mut deployment: Deployment = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(d) => d,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Add default replicas if not specified
    if let Some(spec) = &mut deployment.spec {
        if spec.replicas.is_none() {
            patches.push(json!({
                "op": "add",
                "path": "/spec/replicas",
                "value": 1
            }));
            modified = true;
        }

        // Set default strategy if not specified
        if spec.strategy.is_none() {
            patches.push(json!({
                "op": "add",
                "path": "/spec/strategy",
                "value": {
                    "type": "RollingUpdate",
                    "rollingUpdate": {
                        "maxUnavailable": "25%",
                        "maxSurge": "25%"
                    }
                }
            }));
            modified = true;
        }

        // Add revision history limit if not specified
        if spec.revision_history_limit.is_none() {
            patches.push(json!({
                "op": "add",
                "path": "/spec/revisionHistoryLimit",
                "value": 10
            }));
            modified = true;
        }
    }

    // Add management annotations
    if deployment.metadata.annotations.get("k1s.io/managed-by").is_none() {
        deployment.metadata.annotations.insert(
            "k1s.io/managed-by".to_string(),
            "k1s".to_string(),
        );
        patches.push(json!({
            "op": "add",
            "path": "/metadata/annotations/k1s.io~1managed-by",
            "value": "k1s"
        }));
        modified = true;
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        let patch_base64 = base64::encode(&patch_json);
        debug!("Mutated deployment {} with {} patches", deployment.metadata.name, patches.len());
        AdmissionResponse::mutate(request.uid.clone(), patch_base64)
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

/// Route mutation request to appropriate mutator based on resource kind
/// Mutates DaemonSet resources with defaults
pub fn mutate_daemonset(request: &AdmissionRequest) -> AdmissionResponse {
    let mut daemonset: DaemonSet = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(d) => d,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Ensure annotations object exists
    let annotations_exist = !daemonset.metadata.annotations.is_empty();
    if !annotations_exist && request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("annotations"))
        .is_none()
    {
        patches.push(json!({
            "op": "add",
            "path": "/metadata/annotations",
            "value": {}
        }));
        modified = true;
    }

    // Add management annotations
    if daemonset.metadata.annotations.get("k1s.io/managed-by").is_none() {
        daemonset.metadata.annotations.insert(
            "k1s.io/managed-by".to_string(),
            "k1s".to_string(),
        );
        patches.push(json!({
            "op": "add",
            "path": "/metadata/annotations/k1s.io~1managed-by",
            "value": "k1s"
        }));
        modified = true;
    }

    // Set default update strategy if not specified
    if let Some(spec) = &mut daemonset.spec {
        if spec.update_strategy.is_none() {
            patches.push(json!({
                "op": "add",
                "path": "/spec/updateStrategy",
                "value": {
                    "type": "RollingUpdate",
                    "rollingUpdate": {
                        "maxUnavailable": 1
                    }
                }
            }));
            modified = true;
        }
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        let patch_base64 = base64::encode(&patch_json);
        debug!("Mutated daemonset {} with {} patches", daemonset.metadata.name, patches.len());
        AdmissionResponse::mutate(request.uid.clone(), patch_base64)
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

/// Mutates StatefulSet resources with defaults
pub fn mutate_statefulset(request: &AdmissionRequest) -> AdmissionResponse {
    let mut statefulset: StatefulSet = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(s) => s,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Ensure annotations exist
    if !statefulset.metadata.annotations.is_empty() || request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("annotations"))
        .is_none()
    {
        if statefulset.metadata.annotations.is_empty() {
            patches.push(json!({"op": "add", "path": "/metadata/annotations", "value": {}}));
            modified = true;
        }
    }

    if statefulset.metadata.annotations.get("k1s.io/managed-by").is_none() {
        patches.push(json!({"op": "add", "path": "/metadata/annotations/k1s.io~1managed-by", "value": "k1s"}));
        modified = true;
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        AdmissionResponse::mutate(request.uid.clone(), base64::encode(&patch_json))
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

/// Mutates ReplicaSet resources with defaults
pub fn mutate_replicaset(request: &AdmissionRequest) -> AdmissionResponse {
    let mut replicaset: ReplicaSet = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(r) => r,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Ensure annotations exist
    if !replicaset.metadata.annotations.is_empty() || request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("annotations"))
        .is_none()
    {
        if replicaset.metadata.annotations.is_empty() {
            patches.push(json!({"op": "add", "path": "/metadata/annotations", "value": {}}));
            modified = true;
        }
    }

    if replicaset.metadata.annotations.get("k1s.io/managed-by").is_none() {
        patches.push(json!({"op": "add", "path": "/metadata/annotations/k1s.io~1managed-by", "value": "k1s"}));
        modified = true;
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        AdmissionResponse::mutate(request.uid.clone(), base64::encode(&patch_json))
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

/// Mutates CronJob resources with defaults
pub fn mutate_cronjob(request: &AdmissionRequest) -> AdmissionResponse {
    let mut cronjob: CronJob = match request.object.as_ref().and_then(|obj| serde_json::from_value(obj.clone()).ok()) {
        Some(c) => c,
        None => return AdmissionResponse::allow(request.uid.clone()),
    };

    let mut patches = Vec::new();
    let mut modified = false;

    // Ensure annotations exist
    if !cronjob.metadata.annotations.is_empty() || request.object.as_ref()
        .and_then(|o| o.get("metadata"))
        .and_then(|m| m.get("annotations"))
        .is_none()
    {
        if cronjob.metadata.annotations.is_empty() {
            patches.push(json!({"op": "add", "path": "/metadata/annotations", "value": {}}));
            modified = true;
        }
    }

    if cronjob.metadata.annotations.get("k1s.io/managed-by").is_none() {
        patches.push(json!({"op": "add", "path": "/metadata/annotations/k1s.io~1managed-by", "value": "k1s"}));
        modified = true;
    }

    if modified {
        let patch_json = serde_json::to_string(&patches).unwrap();
        AdmissionResponse::mutate(request.uid.clone(), base64::encode(&patch_json))
    } else {
        AdmissionResponse::allow(request.uid.clone())
    }
}

pub fn mutate_resource(request: &AdmissionRequest) -> AdmissionResponse {
    match request.kind.kind.as_str() {
        "Pod" => mutate_pod(request),
        "Deployment" => mutate_deployment(request),
        "DaemonSet" => mutate_daemonset(request),
        "StatefulSet" => mutate_statefulset(request),
        "ReplicaSet" => mutate_replicaset(request),
        "CronJob" => mutate_cronjob(request),
        _ => {
            // Unknown resource type, allow without mutation
            debug!("No built-in mutator for kind: {}", request.kind.kind);
            AdmissionResponse::allow(request.uid.clone())
        }
    }
}

/// Extract app name from pod name (strip hash suffix)
fn extract_app_name(name: &str) -> Option<String> {
    // Strip common suffixes like -xxxxx or -xxx-xxxxx
    let parts: Vec<&str> = name.rsplitn(2, '-').collect();
    if parts.len() == 2 {
        // Check if the last part looks like a hash (5+ alphanumeric chars)
        let last = parts[0];
        if last.len() >= 5 && last.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Some(parts[1].to_string());
        }
    }
    Some(name.to_string())
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
    fn test_mutate_pod_adds_defaults() {
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
        let response = mutate_pod(&request);
        assert!(response.allowed);
        // Should have mutations for labels, annotations, and resources
        assert!(response.patch.is_some());
    }

    #[test]
    fn test_extract_app_name() {
        assert_eq!(extract_app_name("nginx-abcde"), Some("nginx".to_string()));
        assert_eq!(extract_app_name("my-app-12345"), Some("my-app".to_string()));
        assert_eq!(extract_app_name("simple"), Some("simple".to_string()));
    }
}
