//! API route definitions

use axum::routing::{delete, get, post, put};
use axum::Router;

use crate::handlers::{apps, core, healthz, livez, pod_subresources, readyz};
use crate::state::AppState;

/// Build the complete API router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health endpoints
        .route("/healthz", get(healthz))
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        // API discovery
        .route("/api", get(api_versions))
        .route("/api/v1", get(api_v1_resources))
        .route("/apis", get(api_groups))
        .route("/apis/apps/v1", get(apps_v1_resources))
        // Core v1 API
        .nest("/api/v1", core_v1_routes())
        // Apps v1 API
        .nest("/apis/apps/v1", apps_v1_routes())
        .with_state(state)
}

async fn api_versions() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIVersions",
        "versions": ["v1"],
        "serverAddressByClientCIDRs": [{
            "clientCIDR": "0.0.0.0/0",
            "serverAddress": ""
        }]
    }))
}

async fn api_v1_resources() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIResourceList",
        "groupVersion": "v1",
        "resources": [
            {
                "name": "namespaces",
                "singularName": "namespace",
                "namespaced": false,
                "kind": "Namespace",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "nodes",
                "singularName": "node",
                "namespaced": false,
                "kind": "Node",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "pods",
                "singularName": "pod",
                "namespaced": true,
                "kind": "Pod",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "pods/log",
                "singularName": "",
                "namespaced": true,
                "kind": "Pod",
                "verbs": ["get"]
            },
            {
                "name": "pods/exec",
                "singularName": "",
                "namespaced": true,
                "kind": "Pod",
                "verbs": ["create", "get"]
            },
            {
                "name": "services",
                "singularName": "service",
                "namespaced": true,
                "kind": "Service",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "endpoints",
                "singularName": "endpoint",
                "namespaced": true,
                "kind": "Endpoints",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "configmaps",
                "singularName": "configmap",
                "namespaced": true,
                "kind": "ConfigMap",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "secrets",
                "singularName": "secret",
                "namespaced": true,
                "kind": "Secret",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

async fn api_groups() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIGroupList",
        "apiVersion": "v1",
        "groups": [
            {
                "name": "apps",
                "versions": [
                    {
                        "groupVersion": "apps/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "apps/v1",
                    "version": "v1"
                }
            }
        ]
    }))
}

async fn apps_v1_resources() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIResourceList",
        "groupVersion": "apps/v1",
        "resources": [
            {
                "name": "deployments",
                "singularName": "deployment",
                "namespaced": true,
                "kind": "Deployment",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "replicasets",
                "singularName": "replicaset",
                "namespaced": true,
                "kind": "ReplicaSet",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "daemonsets",
                "singularName": "daemonset",
                "namespaced": true,
                "kind": "DaemonSet",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

/// Core v1 API routes
fn core_v1_routes() -> Router<AppState> {
    Router::new()
        // Namespaces (cluster-scoped)
        .route("/namespaces", get(core::list_namespaces))
        .route("/namespaces", post(core::create_namespace))
        .route("/namespaces/:name", get(core::get_namespace))
        .route("/namespaces/:name", delete(core::delete_namespace))
        // Nodes (cluster-scoped)
        .route("/nodes", get(core::list_nodes))
        .route("/nodes", post(core::create_node))
        .route("/nodes/:name", get(core::get_node))
        .route("/nodes/:name", put(core::update_node))
        .route("/nodes/:name", delete(core::delete_node))
        // Pods (all namespaces)
        .route("/pods", get(core::list_all_pods))
        // Endpoints (all namespaces)
        .route("/endpoints", get(core::list_all_endpoints))
        // Pods (namespaced)
        .route("/namespaces/:namespace/pods", get(core::list_pods))
        .route("/namespaces/:namespace/pods", post(core::create_pod))
        .route("/namespaces/:namespace/pods/:name", get(core::get_pod))
        .route("/namespaces/:namespace/pods/:name", put(core::update_pod))
        .route(
            "/namespaces/:namespace/pods/:name",
            delete(core::delete_pod),
        )
        // Pod subresources
        .route(
            "/namespaces/:namespace/pods/:name/log",
            get(pod_subresources::pod_logs),
        )
        .route(
            "/namespaces/:namespace/pods/:name/exec",
            post(pod_subresources::pod_exec),
        )
        // Services (namespaced)
        .route(
            "/namespaces/:namespace/services",
            get(core::list_services),
        )
        .route(
            "/namespaces/:namespace/services",
            post(core::create_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            get(core::get_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            delete(core::delete_service),
        )
        // Endpoints (namespaced)
        .route(
            "/namespaces/:namespace/endpoints",
            get(core::list_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints",
            post(core::create_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            get(core::get_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            put(core::update_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            delete(core::delete_endpoints),
        )
        // ConfigMaps (namespaced)
        .route(
            "/namespaces/:namespace/configmaps",
            get(core::list_configmaps),
        )
        .route(
            "/namespaces/:namespace/configmaps",
            post(core::create_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            get(core::get_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            put(core::update_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            delete(core::delete_configmap),
        )
        // Secrets (namespaced)
        .route(
            "/namespaces/:namespace/secrets",
            get(core::list_secrets),
        )
        .route(
            "/namespaces/:namespace/secrets",
            post(core::create_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            get(core::get_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            put(core::update_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            delete(core::delete_secret),
        )
}

/// Apps v1 API routes
fn apps_v1_routes() -> Router<AppState> {
    Router::new()
        // ReplicaSets (all namespaces)
        .route("/replicasets", get(apps::list_all_replicasets))
        // ReplicaSets (namespaced)
        .route(
            "/namespaces/:namespace/replicasets",
            get(apps::list_replicasets),
        )
        .route(
            "/namespaces/:namespace/replicasets",
            post(apps::create_replicaset),
        )
        .route(
            "/namespaces/:namespace/replicasets/:name",
            get(apps::get_replicaset),
        )
        .route(
            "/namespaces/:namespace/replicasets/:name",
            put(apps::update_replicaset),
        )
        .route(
            "/namespaces/:namespace/replicasets/:name",
            delete(apps::delete_replicaset),
        )
        // Deployments (all namespaces)
        .route("/deployments", get(apps::list_all_deployments))
        // Deployments (namespaced)
        .route(
            "/namespaces/:namespace/deployments",
            get(apps::list_deployments),
        )
        .route(
            "/namespaces/:namespace/deployments",
            post(apps::create_deployment),
        )
        .route(
            "/namespaces/:namespace/deployments/:name",
            get(apps::get_deployment),
        )
        .route(
            "/namespaces/:namespace/deployments/:name",
            put(apps::update_deployment),
        )
        .route(
            "/namespaces/:namespace/deployments/:name",
            delete(apps::delete_deployment),
        )
}
