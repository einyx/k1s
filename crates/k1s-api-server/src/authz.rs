//! Authorization (RBAC) enforcement middleware
//!
//! Enforces Role-Based Access Control by checking:
//! - ClusterRoleBindings for cluster-scoped resources
//! - RoleBindings for namespace-scoped resources
//! - PolicyRules to determine if an action is allowed

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, warn};

use k1s_storage::backend::ResourceStore;
use k1s_types::rbac_v1::{ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding};

use crate::auth::UserInfo;
use crate::state::AppState;

/// Authorization error response
#[derive(Debug, Serialize)]
struct AuthzError {
    kind: String,
    #[serde(rename = "apiVersion")]
    api_version: String,
    status: String,
    message: String,
    reason: String,
    code: u16,
}

impl AuthzError {
    fn forbidden(message: &str) -> Self {
        Self {
            kind: "Status".to_string(),
            api_version: "v1".to_string(),
            status: "Failure".to_string(),
            message: message.to_string(),
            reason: "Forbidden".to_string(),
            code: 403,
        }
    }
}

/// Request attributes for authorization
#[derive(Debug, Clone)]
pub struct RequestAttributes {
    /// The user making the request
    pub user: UserInfo,
    /// HTTP verb (get, list, create, update, patch, delete, watch)
    pub verb: String,
    /// API group (e.g., "", "apps", "batch")
    pub api_group: String,
    /// Resource type (e.g., "pods", "deployments")
    pub resource: String,
    /// Subresource if any (e.g., "log", "exec", "scale")
    pub subresource: String,
    /// Namespace (empty for cluster-scoped)
    pub namespace: String,
    /// Resource name (empty for list/create)
    pub name: String,
}

/// Paths that bypass authorization
const BYPASS_PATHS: &[&str] = &[
    "/healthz",
    "/livez",
    "/readyz",
    "/openapi/v2",
    "/openapi/v3",
    "/version",
    "/api",
    "/apis",
];

/// Authorization middleware
pub async fn authz_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Bypass authorization for discovery/health endpoints
    if BYPASS_PATHS.iter().any(|p| path == *p || path.starts_with(&format!("{}/", p))) {
        return next.run(request).await;
    }

    // Get user info from request extensions (set by auth middleware)
    let user = request
        .extensions()
        .get::<UserInfo>()
        .cloned()
        .unwrap_or_default();

    // System masters bypass RBAC
    if user.is_admin() {
        debug!("Allowing admin user: {}", user.username);
        return next.run(request).await;
    }

    // Parse request into attributes
    let attrs = match parse_request_attributes(&user, &request) {
        Some(attrs) => attrs,
        None => {
            // If we can't parse, it's likely a discovery endpoint - allow it
            return next.run(request).await;
        }
    };

    // Check authorization
    match check_authorization(&state, &attrs).await {
        Ok(true) => {
            debug!(
                "Authorized: user={} verb={} resource={}/{}",
                attrs.user.username, attrs.verb, attrs.namespace, attrs.resource
            );
            next.run(request).await
        }
        Ok(false) => {
            warn!(
                "Forbidden: user={} verb={} resource={}/{} name={}",
                attrs.user.username, attrs.verb, attrs.namespace, attrs.resource, attrs.name
            );
            (
                StatusCode::FORBIDDEN,
                Json(AuthzError::forbidden(&format!(
                    "User \"{}\" cannot {} {} in namespace \"{}\"",
                    attrs.user.username,
                    attrs.verb,
                    attrs.resource,
                    if attrs.namespace.is_empty() {
                        "cluster-scoped"
                    } else {
                        &attrs.namespace
                    }
                ))),
            )
                .into_response()
        }
        Err(e) => {
            warn!("Authorization error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthzError::forbidden(&format!("Authorization error: {}", e))),
            )
                .into_response()
        }
    }
}

/// Parse HTTP request into authorization attributes
fn parse_request_attributes(user: &UserInfo, request: &Request) -> Option<RequestAttributes> {
    let path = request.uri().path();
    let method = request.method().as_str();

    // Map HTTP method to Kubernetes verb
    let verb = match method {
        "GET" => {
            // Check if it's a list or get
            if path.ends_with('s') || path.contains("?watch=") {
                "list"
            } else {
                "get"
            }
        }
        "POST" => "create",
        "PUT" => "update",
        "PATCH" => "patch",
        "DELETE" => "delete",
        _ => return None,
    };

    // Check for watch query parameter
    let verb = if request.uri().query().map(|q| q.contains("watch=true")).unwrap_or(false) {
        "watch"
    } else {
        verb
    };

    // Parse path to extract API group, resource, namespace, name
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let (api_group, resource, namespace, name, subresource) = parse_api_path(&parts)?;

    Some(RequestAttributes {
        user: user.clone(),
        verb: verb.to_string(),
        api_group,
        resource,
        subresource,
        namespace,
        name,
    })
}

/// Parse API path into components
fn parse_api_path(parts: &[&str]) -> Option<(String, String, String, String, String)> {
    match parts {
        // /api/v1/namespaces/:namespace/:resource/:name/:subresource
        ["api", "v1", "namespaces", namespace, resource, name, subresource] => {
            Some(("".to_string(), resource.to_string(), namespace.to_string(), name.to_string(), subresource.to_string()))
        }
        // /api/v1/namespaces/:namespace/:resource/:name
        ["api", "v1", "namespaces", namespace, resource, name] => {
            Some(("".to_string(), resource.to_string(), namespace.to_string(), name.to_string(), "".to_string()))
        }
        // /api/v1/namespaces/:namespace/:resource
        ["api", "v1", "namespaces", namespace, resource] => {
            Some(("".to_string(), resource.to_string(), namespace.to_string(), "".to_string(), "".to_string()))
        }
        // /api/v1/:resource/:name (cluster-scoped)
        ["api", "v1", resource, name] => {
            Some(("".to_string(), resource.to_string(), "".to_string(), name.to_string(), "".to_string()))
        }
        // /api/v1/:resource (cluster-scoped list)
        ["api", "v1", resource] => {
            Some(("".to_string(), resource.to_string(), "".to_string(), "".to_string(), "".to_string()))
        }
        // /apis/:group/:version/namespaces/:namespace/:resource/:name/:subresource
        ["apis", group, _version, "namespaces", namespace, resource, name, subresource] => {
            Some((group.to_string(), resource.to_string(), namespace.to_string(), name.to_string(), subresource.to_string()))
        }
        // /apis/:group/:version/namespaces/:namespace/:resource/:name
        ["apis", group, _version, "namespaces", namespace, resource, name] => {
            Some((group.to_string(), resource.to_string(), namespace.to_string(), name.to_string(), "".to_string()))
        }
        // /apis/:group/:version/namespaces/:namespace/:resource
        ["apis", group, _version, "namespaces", namespace, resource] => {
            Some((group.to_string(), resource.to_string(), namespace.to_string(), "".to_string(), "".to_string()))
        }
        // /apis/:group/:version/:resource/:name (cluster-scoped)
        ["apis", group, _version, resource, name] => {
            Some((group.to_string(), resource.to_string(), "".to_string(), name.to_string(), "".to_string()))
        }
        // /apis/:group/:version/:resource (cluster-scoped list)
        ["apis", group, _version, resource] => {
            Some((group.to_string(), resource.to_string(), "".to_string(), "".to_string(), "".to_string()))
        }
        _ => None,
    }
}

/// Check if the request is authorized
async fn check_authorization(state: &AppState, attrs: &RequestAttributes) -> Result<bool, String> {
    // Check ClusterRoleBindings first (apply to all namespaces)
    if check_cluster_role_bindings(state, attrs).await? {
        return Ok(true);
    }

    // If namespaced, also check RoleBindings in that namespace
    if !attrs.namespace.is_empty() {
        if check_role_bindings(state, attrs).await? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Check ClusterRoleBindings
async fn check_cluster_role_bindings(state: &AppState, attrs: &RequestAttributes) -> Result<bool, String> {
    let crb_store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    let cr_store = ResourceStore::<ClusterRole>::new(state.storage.clone());

    let bindings = crb_store
        .list(None)
        .await
        .map_err(|e| format!("Failed to list ClusterRoleBindings: {}", e))?;

    for binding in bindings {
        // Check if any subject matches the user
        if !subject_matches(&binding.subjects, &attrs.user) {
            continue;
        }

        // Get the referenced ClusterRole
        if binding.role_ref.kind != "ClusterRole" {
            continue;
        }

        let role = cr_store
            .get(None, &binding.role_ref.name)
            .await
            .map_err(|e| format!("Failed to get ClusterRole: {}", e))?;

        if let Some(role) = role {
            if rules_allow(&role.rules, attrs) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Check RoleBindings in a namespace
async fn check_role_bindings(state: &AppState, attrs: &RequestAttributes) -> Result<bool, String> {
    let rb_store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    let role_store = ResourceStore::<Role>::new(state.storage.clone());
    let cr_store = ResourceStore::<ClusterRole>::new(state.storage.clone());

    let bindings = rb_store
        .list(Some(&attrs.namespace))
        .await
        .map_err(|e| format!("Failed to list RoleBindings: {}", e))?;

    for binding in bindings {
        // Check if any subject matches the user
        if !subject_matches(&binding.subjects, &attrs.user) {
            continue;
        }

        // Get the referenced Role or ClusterRole
        let rules = if binding.role_ref.kind == "ClusterRole" {
            let role = cr_store
                .get(None, &binding.role_ref.name)
                .await
                .map_err(|e| format!("Failed to get ClusterRole: {}", e))?;
            role.map(|r| r.rules)
        } else {
            let role = role_store
                .get(Some(&attrs.namespace), &binding.role_ref.name)
                .await
                .map_err(|e| format!("Failed to get Role: {}", e))?;
            role.map(|r| r.rules)
        };

        if let Some(rules) = rules {
            if rules_allow(&rules, attrs) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Check if any subject matches the user
fn subject_matches(subjects: &[k1s_types::rbac_v1::Subject], user: &UserInfo) -> bool {
    for subject in subjects {
        match subject.kind.as_str() {
            "User" => {
                if subject.name == user.username {
                    return true;
                }
            }
            "Group" => {
                if user.groups.contains(&subject.name) {
                    return true;
                }
            }
            "ServiceAccount" => {
                let sa_username = format!(
                    "system:serviceaccount:{}:{}",
                    subject.namespace.as_deref().unwrap_or("default"),
                    subject.name
                );
                if sa_username == user.username {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Check if policy rules allow the request
fn rules_allow(rules: &[PolicyRule], attrs: &RequestAttributes) -> bool {
    for rule in rules {
        if rule_matches(rule, attrs) {
            return true;
        }
    }
    false
}

/// Check if a single rule matches the request
fn rule_matches(rule: &PolicyRule, attrs: &RequestAttributes) -> bool {
    // Check verbs
    if !rule.verbs.iter().any(|v| v == "*" || v == &attrs.verb) {
        return false;
    }

    // Check API groups
    if !rule.api_groups.iter().any(|g| g == "*" || g == &attrs.api_group) {
        return false;
    }

    // Check resources
    let resource_to_check = if attrs.subresource.is_empty() {
        attrs.resource.clone()
    } else {
        format!("{}/{}", attrs.resource, attrs.subresource)
    };

    if !rule.resources.iter().any(|r| {
        r == "*" || r == &attrs.resource || r == &resource_to_check
    }) {
        return false;
    }

    // Check resource names (if specified in rule)
    if !rule.resource_names.is_empty() && !attrs.name.is_empty() {
        if !rule.resource_names.iter().any(|n| n == &attrs.name) {
            return false;
        }
    }

    true
}

/// Create default RBAC resources for bootstrapping
pub async fn create_default_rbac(state: &AppState) -> Result<(), String> {
    use k1s_types::ObjectMeta;

    let cr_store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    let crb_store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());

    // Create cluster-admin ClusterRole
    if cr_store.get(None, "cluster-admin").await.map_err(|e| e.to_string())?.is_none() {
        let cluster_admin = ClusterRole {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRole".to_string(),
            metadata: ObjectMeta {
                name: "cluster-admin".to_string(),
                ..Default::default()
            },
            rules: vec![PolicyRule {
                verbs: vec!["*".to_string()],
                api_groups: vec!["*".to_string()],
                resources: vec!["*".to_string()],
                resource_names: vec![],
                non_resource_urls: vec!["*".to_string()],
            }],
            aggregation_rule: None,
        };
        cr_store.create(cluster_admin).await.map_err(|e| e.to_string())?;
    }

    // Create cluster-admin ClusterRoleBinding for system:masters group
    if crb_store.get(None, "cluster-admin").await.map_err(|e| e.to_string())?.is_none() {
        let binding = ClusterRoleBinding {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRoleBinding".to_string(),
            metadata: ObjectMeta {
                name: "cluster-admin".to_string(),
                ..Default::default()
            },
            subjects: vec![k1s_types::rbac_v1::Subject {
                kind: "Group".to_string(),
                name: "system:masters".to_string(),
                api_group: Some("rbac.authorization.k8s.io".to_string()),
                namespace: None,
            }],
            role_ref: k1s_types::rbac_v1::RoleRef {
                api_group: "rbac.authorization.k8s.io".to_string(),
                kind: "ClusterRole".to_string(),
                name: "cluster-admin".to_string(),
            },
        };
        crb_store.create(binding).await.map_err(|e| e.to_string())?;
    }

    // Create system:discovery ClusterRole for API discovery
    if cr_store.get(None, "system:discovery").await.map_err(|e| e.to_string())?.is_none() {
        let discovery = ClusterRole {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRole".to_string(),
            metadata: ObjectMeta {
                name: "system:discovery".to_string(),
                ..Default::default()
            },
            rules: vec![PolicyRule {
                verbs: vec!["get".to_string()],
                api_groups: vec![],
                resources: vec![],
                resource_names: vec![],
                non_resource_urls: vec![
                    "/api".to_string(),
                    "/api/*".to_string(),
                    "/apis".to_string(),
                    "/apis/*".to_string(),
                    "/healthz".to_string(),
                    "/livez".to_string(),
                    "/readyz".to_string(),
                    "/version".to_string(),
                    "/openapi/*".to_string(),
                ],
            }],
            aggregation_rule: None,
        };
        cr_store.create(discovery).await.map_err(|e| e.to_string())?;
    }

    // Bind system:discovery to system:authenticated group
    if crb_store.get(None, "system:discovery").await.map_err(|e| e.to_string())?.is_none() {
        let binding = ClusterRoleBinding {
            api_version: "rbac.authorization.k8s.io/v1".to_string(),
            kind: "ClusterRoleBinding".to_string(),
            metadata: ObjectMeta {
                name: "system:discovery".to_string(),
                ..Default::default()
            },
            subjects: vec![k1s_types::rbac_v1::Subject {
                kind: "Group".to_string(),
                name: "system:authenticated".to_string(),
                api_group: Some("rbac.authorization.k8s.io".to_string()),
                namespace: None,
            }],
            role_ref: k1s_types::rbac_v1::RoleRef {
                api_group: "rbac.authorization.k8s.io".to_string(),
                kind: "ClusterRole".to_string(),
                name: "system:discovery".to_string(),
            },
        };
        crb_store.create(binding).await.map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_path_core_namespaced() {
        let parts = vec!["api", "v1", "namespaces", "default", "pods", "nginx"];
        let result = parse_api_path(&parts).unwrap();
        assert_eq!(result.0, ""); // api_group
        assert_eq!(result.1, "pods"); // resource
        assert_eq!(result.2, "default"); // namespace
        assert_eq!(result.3, "nginx"); // name
    }

    #[test]
    fn test_parse_api_path_apps() {
        let parts = vec!["apis", "apps", "v1", "namespaces", "kube-system", "deployments"];
        let result = parse_api_path(&parts).unwrap();
        assert_eq!(result.0, "apps");
        assert_eq!(result.1, "deployments");
        assert_eq!(result.2, "kube-system");
        assert_eq!(result.3, "");
    }

    #[test]
    fn test_rule_matches_wildcard() {
        let rule = PolicyRule {
            verbs: vec!["*".to_string()],
            api_groups: vec!["*".to_string()],
            resources: vec!["*".to_string()],
            resource_names: vec![],
            non_resource_urls: vec![],
        };
        let attrs = RequestAttributes {
            user: UserInfo::default(),
            verb: "get".to_string(),
            api_group: "apps".to_string(),
            resource: "deployments".to_string(),
            subresource: "".to_string(),
            namespace: "default".to_string(),
            name: "nginx".to_string(),
        };
        assert!(rule_matches(&rule, &attrs));
    }

    #[test]
    fn test_rule_matches_specific() {
        let rule = PolicyRule {
            verbs: vec!["get".to_string(), "list".to_string()],
            api_groups: vec!["".to_string()],
            resources: vec!["pods".to_string()],
            resource_names: vec![],
            non_resource_urls: vec![],
        };
        let attrs = RequestAttributes {
            user: UserInfo::default(),
            verb: "get".to_string(),
            api_group: "".to_string(),
            resource: "pods".to_string(),
            subresource: "".to_string(),
            namespace: "default".to_string(),
            name: "nginx".to_string(),
        };
        assert!(rule_matches(&rule, &attrs));
    }

    #[test]
    fn test_rule_no_match() {
        let rule = PolicyRule {
            verbs: vec!["get".to_string()],
            api_groups: vec!["".to_string()],
            resources: vec!["pods".to_string()],
            resource_names: vec![],
            non_resource_urls: vec![],
        };
        let attrs = RequestAttributes {
            user: UserInfo::default(),
            verb: "delete".to_string(), // Not allowed
            api_group: "".to_string(),
            resource: "pods".to_string(),
            subresource: "".to_string(),
            namespace: "default".to_string(),
            name: "nginx".to_string(),
        };
        assert!(!rule_matches(&rule, &attrs));
    }
}
