//! Admission webhook middleware
//!
//! Middleware that intercepts resource creation/update operations and invokes admission webhooks

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use k1s_storage::SledBackend;
use k1s_types::api::admission::v1::{
    AdmissionRequest, GroupVersionKind, GroupVersionResource, Operation, UserInfo,
};
use serde_json::Value;
use tracing::{debug, error};
use uuid::Uuid;

use super::invoker::WebhookInvoker;
use super::{validate_resource, mutate_resource};
use crate::error::ApiError;

/// Shared state for webhook middleware
#[derive(Clone)]
pub struct WebhookState {
    pub storage: Arc<SledBackend>,
    pub invoker: Arc<WebhookInvoker>,
    pub enable_built_in_validators: bool,
    pub enable_built_in_mutators: bool,
}

impl WebhookState {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self {
            invoker: Arc::new(WebhookInvoker::new(storage.clone())),
            storage,
            enable_built_in_validators: true,
            enable_built_in_mutators: true,
        }
    }

    pub fn with_built_in_validators(mut self, enabled: bool) -> Self {
        self.enable_built_in_validators = enabled;
        self
    }

    pub fn with_built_in_mutators(mut self, enabled: bool) -> Self {
        self.enable_built_in_mutators = enabled;
        self
    }
}

/// Admission webhook middleware that intercepts resource operations
pub async fn admission_webhook_middleware(
    State(webhook_state): State<WebhookState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    // Extract request details
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path();

    tracing::info!("🔍 Webhook middleware called: {} {}", method, path);

    // Only intercept resource creation and updates
    let operation = match method.as_str() {
        "POST" => Operation::Create,
        "PUT" | "PATCH" => Operation::Update,
        "DELETE" => Operation::Delete,
        _ => {
            // Pass through other requests
            return Ok(next.run(request).await);
        }
    };

    // Parse resource type from path
    let (group, version, resource, kind) = match parse_resource_from_path(path) {
        Some(r) => r,
        None => {
            // Not a resource API path, pass through
            return Ok(next.run(request).await);
        }
    };

    // Check Content-Type - only process JSON requests
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    if !content_type.contains("application/json") && !content_type.is_empty() {
        // Not JSON (might be protobuf), pass through
        debug!("Skipping webhook for non-JSON content-type: {}", content_type);
        return Ok(next.run(request).await);
    }

    // Extract namespace from path if present
    let namespace = extract_namespace_from_path(path);

    // Get the request body (take ownership and replace with empty body)
    let body = std::mem::replace(request.body_mut(), axum::body::Body::empty());
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to read request body: {}", e);
            // Reconstruct empty body and pass through
            *request.body_mut() = axum::body::Body::empty();
            return Ok(next.run(request).await);
        }
    };

    // Try to parse as JSON
    let request_body: Value = match serde_json::from_slice(&body_bytes) {
        Ok(body) => body,
        Err(e) => {
            debug!("Failed to parse request body as JSON: {}", e);
            // Not valid JSON, reconstruct body and pass through
            *request.body_mut() = axum::body::Body::from(body_bytes);
            return Ok(next.run(request).await);
        }
    };

    // Build admission request
    let admission_request = AdmissionRequest {
        uid: Uuid::new_v4().to_string(),
        kind: GroupVersionKind {
            group: group.clone(),
            version: version.clone(),
            kind: kind.clone(),
        },
        resource: GroupVersionResource {
            group: group.clone(),
            version: version.clone(),
            resource: resource.clone(),
        },
        sub_resource: None,
        request_kind: None,
        request_resource: None,
        name: request_body.get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
        namespace,
        operation,
        user_info: extract_user_info(&request),
        object: Some(request_body.clone()),
        old_object: None, // TODO: Fetch old object for UPDATE operations
        dry_run: None,
        options: None,
    };

    debug!(
        "Admission request for {} {}/{} (operation: {:?})",
        admission_request.kind.kind,
        admission_request.namespace.as_deref().unwrap_or("cluster"),
        admission_request.name.as_deref().unwrap_or("<unknown>"),
        admission_request.operation
    );

    // Phase 1: Call mutating webhooks (built-in + external)
    if webhook_state.enable_built_in_mutators {
        let mutation_response = mutate_resource(&admission_request);
        if !mutation_response.allowed {
            error!("Built-in mutator denied request: {:?}", mutation_response.status);
            return Err(ApiError::BadRequest(
                mutation_response.status
                    .and_then(|s| s.message)
                    .unwrap_or_else(|| "Mutation webhook denied request".to_string()),
            ));
        }

        // TODO: Apply patches if present
        if let Some(_patch) = mutation_response.patch {
            debug!("Built-in mutator returned patches (not yet applied)");
        }
    }

    // Call external mutating webhooks
    match webhook_state.invoker.mutate(&admission_request).await {
        Ok(response) => {
            if !response.allowed {
                error!("External mutating webhook denied request: {:?}", response.status);
                return Err(ApiError::BadRequest(
                    response.status
                        .and_then(|s| s.message)
                        .unwrap_or_else(|| "Mutating webhook denied request".to_string()),
                ));
            }
            // TODO: Apply patches
        }
        Err(e) => {
            error!("Failed to call mutating webhooks: {}", e);
            return Err(ApiError::Internal(format!("Webhook error: {}", e)));
        }
    }

    // Phase 2: Call validating webhooks (built-in + external)
    if webhook_state.enable_built_in_validators {
        let validation_response = validate_resource(&admission_request);
        if !validation_response.allowed {
            error!("Built-in validator denied request: {:?}", validation_response.status);
            return Err(ApiError::BadRequest(
                validation_response.status
                    .and_then(|s| s.message)
                    .unwrap_or_else(|| "Validation failed".to_string()),
            ));
        }

        // Log warnings
        if let Some(warnings) = validation_response.warnings {
            for warning in warnings {
                debug!("Validation warning: {}", warning);
            }
        }
    }

    // Call external validating webhooks
    match webhook_state.invoker.validate(&admission_request).await {
        Ok(response) => {
            if !response.allowed {
                error!("External validating webhook denied request: {:?}", response.status);
                return Err(ApiError::BadRequest(
                    response.status
                        .and_then(|s| s.message)
                        .unwrap_or_else(|| "Validation webhook denied request".to_string()),
                ));
            }
        }
        Err(e) => {
            error!("Failed to call validating webhooks: {}", e);
            return Err(ApiError::Internal(format!("Webhook error: {}", e)));
        }
    }

    // Webhooks passed, continue with the request
    debug!("Admission webhooks passed for {}", admission_request.kind.kind);

    // Reconstruct the request with the original body
    *request.body_mut() = axum::body::Body::from(body_bytes);

    Ok(next.run(request).await)
}

/// Parse resource type from API path
/// Returns: (group, version, resource, kind)
fn parse_resource_from_path(path: &str) -> Option<(String, String, String, String)> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();

    // Match patterns:
    // /api/v1/pods -> ("", "v1", "pods", "Pod")
    // /api/v1/namespaces/{ns}/pods -> ("", "v1", "pods", "Pod")
    // /apis/apps/v1/deployments -> ("apps", "v1", "deployments", "Deployment")
    // /apis/apps/v1/namespaces/{ns}/deployments -> ("apps", "v1", "deployments", "Deployment")

    match parts.as_slice() {
        // /api/v1/pods
        ["api", version, resource] => {
            let kind = resource_to_kind(resource)?;
            Some(("".to_string(), version.to_string(), resource.to_string(), kind))
        }
        // /api/v1/namespaces/{ns}/pods
        ["api", version, "namespaces", _ns, resource] => {
            let kind = resource_to_kind(resource)?;
            Some(("".to_string(), version.to_string(), resource.to_string(), kind))
        }
        // /apis/apps/v1/deployments
        ["apis", group, version, resource] => {
            let kind = resource_to_kind(resource)?;
            Some((group.to_string(), version.to_string(), resource.to_string(), kind))
        }
        // /apis/apps/v1/namespaces/{ns}/deployments
        ["apis", group, version, "namespaces", _ns, resource] => {
            let kind = resource_to_kind(resource)?;
            Some((group.to_string(), version.to_string(), resource.to_string(), kind))
        }
        _ => None,
    }
}

/// Convert resource plural to kind singular
fn resource_to_kind(resource: &str) -> Option<String> {
    let kind = match resource {
        "pods" => "Pod",
        "services" => "Service",
        "configmaps" => "ConfigMap",
        "secrets" => "Secret",
        "serviceaccounts" => "ServiceAccount",
        "persistentvolumes" => "PersistentVolume",
        "persistentvolumeclaims" => "PersistentVolumeClaim",
        "namespaces" => "Namespace",
        "nodes" => "Node",
        "endpoints" => "Endpoints",
        "events" => "Event",
        // apps
        "deployments" => "Deployment",
        "replicasets" => "ReplicaSet",
        "statefulsets" => "StatefulSet",
        "daemonsets" => "DaemonSet",
        // batch
        "jobs" => "Job",
        "cronjobs" => "CronJob",
        // rbac
        "roles" => "Role",
        "rolebindings" => "RoleBinding",
        "clusterroles" => "ClusterRole",
        "clusterrolebindings" => "ClusterRoleBinding",
        // storage
        "storageclasses" => "StorageClass",
        _ => return None,
    };
    Some(kind.to_string())
}

/// Extract namespace from path
fn extract_namespace_from_path(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    for i in 0..parts.len() {
        if parts[i] == "namespaces" && i + 1 < parts.len() {
            return Some(parts[i + 1].to_string());
        }
    }
    None
}

/// Extract user info from request headers
fn extract_user_info(request: &Request) -> UserInfo {
    // Extract from authentication headers (simplified)
    let username = request
        .headers()
        .get("X-Remote-User")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("system:anonymous")
        .to_string();

    let groups = request
        .headers()
        .get("X-Remote-Group")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').map(|g| g.trim().to_string()).collect())
        .unwrap_or_else(|| vec!["system:unauthenticated".to_string()]);

    UserInfo {
        username,
        uid: None,
        groups,
        extra: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_resource_from_path() {
        let (group, version, resource, kind) = parse_resource_from_path("/api/v1/pods").unwrap();
        assert_eq!(group, "");
        assert_eq!(version, "v1");
        assert_eq!(resource, "pods");
        assert_eq!(kind, "Pod");

        let (group, version, resource, kind) = parse_resource_from_path("/apis/apps/v1/deployments").unwrap();
        assert_eq!(group, "apps");
        assert_eq!(version, "v1");
        assert_eq!(resource, "deployments");
        assert_eq!(kind, "Deployment");

        let (group, version, resource, kind) = parse_resource_from_path("/api/v1/namespaces/default/pods").unwrap();
        assert_eq!(group, "");
        assert_eq!(version, "v1");
        assert_eq!(resource, "pods");
        assert_eq!(kind, "Pod");
    }

    #[test]
    fn test_extract_namespace_from_path() {
        assert_eq!(
            extract_namespace_from_path("/api/v1/namespaces/default/pods"),
            Some("default".to_string())
        );
        assert_eq!(
            extract_namespace_from_path("/apis/apps/v1/namespaces/kube-system/deployments"),
            Some("kube-system".to_string())
        );
        assert_eq!(extract_namespace_from_path("/api/v1/pods"), None);
    }

    #[test]
    fn test_resource_to_kind() {
        assert_eq!(resource_to_kind("pods"), Some("Pod".to_string()));
        assert_eq!(resource_to_kind("deployments"), Some("Deployment".to_string()));
        assert_eq!(resource_to_kind("unknown"), None);
    }
}
