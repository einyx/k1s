//! API route definitions

use axum::routing::{delete, get, patch, post, put};
use axum::Router;

use crate::handlers::{apps, batch, core, events, healthz, list_watch, livez, openapi, pod_subresources, rbac, readyz, scale, storage};
use crate::state::AppState;

/// Build the complete API router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health endpoints
        .route("/healthz", get(healthz))
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        // OpenAPI schema
        .route("/openapi/v2", get(openapi::openapi_v2))
        .route("/openapi/v3", get(openapi::openapi_v3))
        // API discovery
        .route("/api", get(api_versions))
        .route("/api/v1", get(api_v1_resources))
        .route("/apis", get(api_groups))
        .route("/apis/apps/v1", get(apps_v1_resources))
        .route("/apis/batch/v1", get(batch_v1_resources))
        .route("/apis/rbac.authorization.k8s.io/v1", get(rbac_v1_resources))
        .route("/apis/storage.k8s.io/v1", get(storage_v1_resources))
        // Core v1 API
        .nest("/api/v1", core_v1_routes())
        // Apps v1 API
        .nest("/apis/apps/v1", apps_v1_routes())
        // Batch v1 API
        .nest("/apis/batch/v1", batch_v1_routes())
        // RBAC v1 API
        .nest("/apis/rbac.authorization.k8s.io/v1", rbac_v1_routes())
        // Storage v1 API
        .nest("/apis/storage.k8s.io/v1", storage_v1_routes())
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
                "shortNames": ["ns"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "nodes",
                "singularName": "node",
                "namespaced": false,
                "kind": "Node",
                "shortNames": ["no"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "pods",
                "singularName": "pod",
                "namespaced": true,
                "kind": "Pod",
                "shortNames": ["po"],
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
                "shortNames": ["svc"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "endpoints",
                "singularName": "endpoint",
                "namespaced": true,
                "kind": "Endpoints",
                "shortNames": ["ep"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "configmaps",
                "singularName": "configmap",
                "namespaced": true,
                "kind": "ConfigMap",
                "shortNames": ["cm"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "secrets",
                "singularName": "secret",
                "namespaced": true,
                "kind": "Secret",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "serviceaccounts",
                "singularName": "serviceaccount",
                "namespaced": true,
                "kind": "ServiceAccount",
                "shortNames": ["sa"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "persistentvolumes",
                "singularName": "persistentvolume",
                "namespaced": false,
                "kind": "PersistentVolume",
                "shortNames": ["pv"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "persistentvolumeclaims",
                "singularName": "persistentvolumeclaim",
                "namespaced": true,
                "kind": "PersistentVolumeClaim",
                "shortNames": ["pvc"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "events",
                "singularName": "event",
                "namespaced": true,
                "kind": "Event",
                "shortNames": ["ev"],
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
            },
            {
                "name": "batch",
                "versions": [
                    {
                        "groupVersion": "batch/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "batch/v1",
                    "version": "v1"
                }
            },
            {
                "name": "rbac.authorization.k8s.io",
                "versions": [
                    {
                        "groupVersion": "rbac.authorization.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "rbac.authorization.k8s.io/v1",
                    "version": "v1"
                }
            },
            {
                "name": "storage.k8s.io",
                "versions": [
                    {
                        "groupVersion": "storage.k8s.io/v1",
                        "version": "v1"
                    }
                ],
                "preferredVersion": {
                    "groupVersion": "storage.k8s.io/v1",
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
                "shortNames": ["deploy"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "deployments/scale",
                "singularName": "",
                "namespaced": true,
                "kind": "Scale",
                "group": "autoscaling",
                "version": "v1",
                "verbs": ["get", "patch", "update"]
            },
            {
                "name": "replicasets",
                "singularName": "replicaset",
                "namespaced": true,
                "kind": "ReplicaSet",
                "shortNames": ["rs"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "replicasets/scale",
                "singularName": "",
                "namespaced": true,
                "kind": "Scale",
                "group": "autoscaling",
                "version": "v1",
                "verbs": ["get", "patch", "update"]
            },
            {
                "name": "daemonsets",
                "singularName": "daemonset",
                "namespaced": true,
                "kind": "DaemonSet",
                "shortNames": ["ds"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "statefulsets",
                "singularName": "statefulset",
                "namespaced": true,
                "kind": "StatefulSet",
                "shortNames": ["sts"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "statefulsets/scale",
                "singularName": "",
                "namespaced": true,
                "kind": "Scale",
                "group": "autoscaling",
                "version": "v1",
                "verbs": ["get", "patch", "update"]
            }
        ]
    }))
}

async fn batch_v1_resources() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIResourceList",
        "groupVersion": "batch/v1",
        "resources": [
            {
                "name": "jobs",
                "singularName": "job",
                "namespaced": true,
                "kind": "Job",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "cronjobs",
                "singularName": "cronjob",
                "namespaced": true,
                "kind": "CronJob",
                "shortNames": ["cj"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

async fn rbac_v1_resources() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIResourceList",
        "groupVersion": "rbac.authorization.k8s.io/v1",
        "resources": [
            {
                "name": "roles",
                "singularName": "role",
                "namespaced": true,
                "kind": "Role",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "clusterroles",
                "singularName": "clusterrole",
                "namespaced": false,
                "kind": "ClusterRole",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "rolebindings",
                "singularName": "rolebinding",
                "namespaced": true,
                "kind": "RoleBinding",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            },
            {
                "name": "clusterrolebindings",
                "singularName": "clusterrolebinding",
                "namespaced": false,
                "kind": "ClusterRoleBinding",
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

async fn storage_v1_resources() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "kind": "APIResourceList",
        "groupVersion": "storage.k8s.io/v1",
        "resources": [
            {
                "name": "storageclasses",
                "singularName": "storageclass",
                "namespaced": false,
                "kind": "StorageClass",
                "shortNames": ["sc"],
                "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
            }
        ]
    }))
}

/// Core v1 API routes
fn core_v1_routes() -> Router<AppState> {
    Router::new()
        // Namespaces (cluster-scoped) - with watch support
        .route("/namespaces", get(list_watch::list_namespaces))
        .route("/namespaces", post(core::create_namespace))
        .route("/namespaces/:name", get(core::get_namespace))
        .route("/namespaces/:name", patch(core::patch_namespace))
        .route("/namespaces/:name", delete(core::delete_namespace))
        // Nodes (cluster-scoped) - with watch support
        .route("/nodes", get(list_watch::list_nodes))
        .route("/nodes", post(core::create_node))
        .route("/nodes/:name", get(core::get_node))
        .route("/nodes/:name", put(core::update_node))
        .route("/nodes/:name", patch(core::patch_node))
        .route("/nodes/:name", delete(core::delete_node))
        // Pods (all namespaces) - with watch support
        .route("/pods", get(list_watch::list_all_pods))
        // Endpoints (all namespaces) - with watch support
        .route("/endpoints", get(list_watch::list_all_endpoints))
        // Pods (namespaced) - with watch support
        .route("/namespaces/:namespace/pods", get(list_watch::list_pods))
        .route("/namespaces/:namespace/pods", post(core::create_pod))
        .route("/namespaces/:namespace/pods/:name", get(core::get_pod))
        .route("/namespaces/:namespace/pods/:name", put(core::update_pod))
        .route("/namespaces/:namespace/pods/:name", patch(core::patch_pod))
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
        // Services (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/services",
            get(list_watch::list_services),
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
            patch(core::patch_service),
        )
        .route(
            "/namespaces/:namespace/services/:name",
            delete(core::delete_service),
        )
        // Endpoints (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/endpoints",
            get(list_watch::list_endpoints),
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
            patch(core::patch_endpoints),
        )
        .route(
            "/namespaces/:namespace/endpoints/:name",
            delete(core::delete_endpoints),
        )
        // ConfigMaps (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/configmaps",
            get(list_watch::list_configmaps),
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
            patch(core::patch_configmap),
        )
        .route(
            "/namespaces/:namespace/configmaps/:name",
            delete(core::delete_configmap),
        )
        // Secrets (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/secrets",
            get(list_watch::list_secrets),
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
            patch(core::patch_secret),
        )
        .route(
            "/namespaces/:namespace/secrets/:name",
            delete(core::delete_secret),
        )
        // PersistentVolumes (cluster-scoped)
        .route("/persistentvolumes", get(storage::list_persistentvolumes))
        .route("/persistentvolumes", post(storage::create_persistentvolume))
        .route("/persistentvolumes/:name", get(storage::get_persistentvolume))
        .route("/persistentvolumes/:name", put(storage::update_persistentvolume))
        .route("/persistentvolumes/:name", patch(storage::patch_persistentvolume))
        .route("/persistentvolumes/:name", delete(storage::delete_persistentvolume))
        // PersistentVolumeClaims (all namespaces)
        .route("/persistentvolumeclaims", get(storage::list_all_pvcs))
        // PersistentVolumeClaims (namespaced)
        .route(
            "/namespaces/:namespace/persistentvolumeclaims",
            get(storage::list_pvcs),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims",
            post(storage::create_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            get(storage::get_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            put(storage::update_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            patch(storage::patch_pvc),
        )
        .route(
            "/namespaces/:namespace/persistentvolumeclaims/:name",
            delete(storage::delete_pvc),
        )
        // ServiceAccounts (namespaced)
        .route(
            "/namespaces/:namespace/serviceaccounts",
            get(storage::list_serviceaccounts),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts",
            post(storage::create_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            get(storage::get_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            put(storage::update_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            patch(storage::patch_serviceaccount),
        )
        .route(
            "/namespaces/:namespace/serviceaccounts/:name",
            delete(storage::delete_serviceaccount),
        )
        // Events (all namespaces)
        .route("/events", get(events::list_all_events))
        // Events (namespaced)
        .route(
            "/namespaces/:namespace/events",
            get(events::list_events),
        )
        .route(
            "/namespaces/:namespace/events",
            post(events::create_event),
        )
        .route(
            "/namespaces/:namespace/events/:name",
            get(events::get_event),
        )
        .route(
            "/namespaces/:namespace/events/:name",
            put(events::update_event),
        )
        .route(
            "/namespaces/:namespace/events/:name",
            patch(events::patch_event),
        )
        .route(
            "/namespaces/:namespace/events/:name",
            delete(events::delete_event),
        )
}

/// Apps v1 API routes
fn apps_v1_routes() -> Router<AppState> {
    Router::new()
        // ReplicaSets (all namespaces) - with watch support
        .route("/replicasets", get(list_watch::list_all_replicasets))
        // ReplicaSets (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/replicasets",
            get(list_watch::list_replicasets),
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
            patch(apps::patch_replicaset),
        )
        .route(
            "/namespaces/:namespace/replicasets/:name",
            delete(apps::delete_replicaset),
        )
        // Deployments (all namespaces) - with watch support
        .route("/deployments", get(list_watch::list_all_deployments))
        // Deployments (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/deployments",
            get(list_watch::list_deployments),
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
            patch(apps::patch_deployment),
        )
        .route(
            "/namespaces/:namespace/deployments/:name",
            delete(apps::delete_deployment),
        )
        // Deployment scale subresource
        .route(
            "/namespaces/:namespace/deployments/:name/scale",
            get(scale::get_deployment_scale),
        )
        .route(
            "/namespaces/:namespace/deployments/:name/scale",
            put(scale::update_deployment_scale),
        )
        // ReplicaSet scale subresource
        .route(
            "/namespaces/:namespace/replicasets/:name/scale",
            get(scale::get_replicaset_scale),
        )
        .route(
            "/namespaces/:namespace/replicasets/:name/scale",
            put(scale::update_replicaset_scale),
        )
        // DaemonSets (all namespaces) - with watch support
        .route("/daemonsets", get(list_watch::list_all_daemonsets))
        // DaemonSets (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/daemonsets",
            get(list_watch::list_daemonsets),
        )
        .route(
            "/namespaces/:namespace/daemonsets",
            post(apps::create_daemonset),
        )
        .route(
            "/namespaces/:namespace/daemonsets/:name",
            get(apps::get_daemonset),
        )
        .route(
            "/namespaces/:namespace/daemonsets/:name",
            put(apps::update_daemonset),
        )
        .route(
            "/namespaces/:namespace/daemonsets/:name",
            patch(apps::patch_daemonset),
        )
        .route(
            "/namespaces/:namespace/daemonsets/:name",
            delete(apps::delete_daemonset),
        )
        // StatefulSets (all namespaces)
        .route("/statefulsets", get(apps::list_all_statefulsets))
        // StatefulSets (namespaced)
        .route(
            "/namespaces/:namespace/statefulsets",
            get(apps::list_statefulsets),
        )
        .route(
            "/namespaces/:namespace/statefulsets",
            post(apps::create_statefulset),
        )
        .route(
            "/namespaces/:namespace/statefulsets/:name",
            get(apps::get_statefulset),
        )
        .route(
            "/namespaces/:namespace/statefulsets/:name",
            put(apps::update_statefulset),
        )
        .route(
            "/namespaces/:namespace/statefulsets/:name",
            patch(apps::patch_statefulset),
        )
        .route(
            "/namespaces/:namespace/statefulsets/:name",
            delete(apps::delete_statefulset),
        )
        // StatefulSet scale subresource
        .route(
            "/namespaces/:namespace/statefulsets/:name/scale",
            get(scale::get_statefulset_scale),
        )
        .route(
            "/namespaces/:namespace/statefulsets/:name/scale",
            put(scale::update_statefulset_scale),
        )
}

/// Batch v1 API routes
fn batch_v1_routes() -> Router<AppState> {
    Router::new()
        // Jobs (all namespaces) - with watch support
        .route("/jobs", get(list_watch::list_all_jobs))
        // Jobs (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/jobs",
            get(list_watch::list_jobs),
        )
        .route(
            "/namespaces/:namespace/jobs",
            post(batch::create_job),
        )
        .route(
            "/namespaces/:namespace/jobs/:name",
            get(batch::get_job),
        )
        .route(
            "/namespaces/:namespace/jobs/:name",
            put(batch::update_job),
        )
        .route(
            "/namespaces/:namespace/jobs/:name",
            patch(batch::patch_job),
        )
        .route(
            "/namespaces/:namespace/jobs/:name",
            delete(batch::delete_job),
        )
        // CronJobs (all namespaces) - with watch support
        .route("/cronjobs", get(list_watch::list_all_cronjobs))
        // CronJobs (namespaced) - with watch support
        .route(
            "/namespaces/:namespace/cronjobs",
            get(list_watch::list_cronjobs),
        )
        .route(
            "/namespaces/:namespace/cronjobs",
            post(batch::create_cronjob),
        )
        .route(
            "/namespaces/:namespace/cronjobs/:name",
            get(batch::get_cronjob),
        )
        .route(
            "/namespaces/:namespace/cronjobs/:name",
            put(batch::update_cronjob),
        )
        .route(
            "/namespaces/:namespace/cronjobs/:name",
            patch(batch::patch_cronjob),
        )
        .route(
            "/namespaces/:namespace/cronjobs/:name",
            delete(batch::delete_cronjob),
        )
}

/// RBAC v1 API routes
fn rbac_v1_routes() -> Router<AppState> {
    Router::new()
        // Roles (namespaced)
        .route("/namespaces/:namespace/roles", get(rbac::list_roles))
        .route("/namespaces/:namespace/roles", post(rbac::create_role))
        .route("/namespaces/:namespace/roles/:name", get(rbac::get_role))
        .route("/namespaces/:namespace/roles/:name", put(rbac::update_role))
        .route("/namespaces/:namespace/roles/:name", patch(rbac::patch_role))
        .route("/namespaces/:namespace/roles/:name", delete(rbac::delete_role))
        // ClusterRoles (cluster-scoped)
        .route("/clusterroles", get(rbac::list_clusterroles))
        .route("/clusterroles", post(rbac::create_clusterrole))
        .route("/clusterroles/:name", get(rbac::get_clusterrole))
        .route("/clusterroles/:name", put(rbac::update_clusterrole))
        .route("/clusterroles/:name", patch(rbac::patch_clusterrole))
        .route("/clusterroles/:name", delete(rbac::delete_clusterrole))
        // RoleBindings (namespaced)
        .route(
            "/namespaces/:namespace/rolebindings",
            get(rbac::list_rolebindings),
        )
        .route(
            "/namespaces/:namespace/rolebindings",
            post(rbac::create_rolebinding),
        )
        .route(
            "/namespaces/:namespace/rolebindings/:name",
            get(rbac::get_rolebinding),
        )
        .route(
            "/namespaces/:namespace/rolebindings/:name",
            put(rbac::update_rolebinding),
        )
        .route(
            "/namespaces/:namespace/rolebindings/:name",
            patch(rbac::patch_rolebinding),
        )
        .route(
            "/namespaces/:namespace/rolebindings/:name",
            delete(rbac::delete_rolebinding),
        )
        // ClusterRoleBindings (cluster-scoped)
        .route("/clusterrolebindings", get(rbac::list_clusterrolebindings))
        .route("/clusterrolebindings", post(rbac::create_clusterrolebinding))
        .route("/clusterrolebindings/:name", get(rbac::get_clusterrolebinding))
        .route("/clusterrolebindings/:name", put(rbac::update_clusterrolebinding))
        .route("/clusterrolebindings/:name", patch(rbac::patch_clusterrolebinding))
        .route("/clusterrolebindings/:name", delete(rbac::delete_clusterrolebinding))
}

/// Storage v1 API routes
fn storage_v1_routes() -> Router<AppState> {
    Router::new()
        // StorageClasses (cluster-scoped)
        .route("/storageclasses", get(storage::list_storageclasses))
        .route("/storageclasses", post(storage::create_storageclass))
        .route("/storageclasses/:name", get(storage::get_storageclass))
        .route("/storageclasses/:name", put(storage::update_storageclass))
        .route("/storageclasses/:name", patch(storage::patch_storageclass))
        .route("/storageclasses/:name", delete(storage::delete_storageclass))
}
