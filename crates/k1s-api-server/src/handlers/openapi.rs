//! OpenAPI schema generation for kubectl validation

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

/// Generate a minimal OpenAPI v2 (Swagger) schema for k1s resources
/// This provides enough information for kubectl to validate resources
/// Note: kubectl may request protobuf format, but we always return JSON which it can handle
pub async fn openapi_v2(headers: HeaderMap) -> impl IntoResponse {
    // If the client only accepts protobuf, return 406 so it retries with JSON.
    // kubectl prefers protobuf but falls back to JSON on Not Acceptable.
    let accept = headers.get("accept").and_then(|h| h.to_str().ok()).unwrap_or("");
    if accept.contains("protobuf") && !accept.contains("application/json") && !accept.contains("*/*") {
        return Response::builder()
            .status(StatusCode::NOT_ACCEPTABLE)
            .body(axum::body::Body::empty())
            .unwrap();
    }

    let schema = json!({
        "swagger": "2.0",
        "info": {
            "title": "k1s API",
            "version": "v1"
        },
        "paths": {},
        "definitions": {
            // Core v1 types
            "io.k8s.api.core.v1.Pod": pod_schema(),
            "io.k8s.api.core.v1.PodSpec": pod_spec_schema(),
            "io.k8s.api.core.v1.PodStatus": pod_status_schema(),
            "io.k8s.api.core.v1.Container": container_schema(),
            "io.k8s.api.core.v1.Namespace": namespace_schema(),
            "io.k8s.api.core.v1.Node": node_schema(),
            "io.k8s.api.core.v1.Service": service_schema(),
            "io.k8s.api.core.v1.ServiceSpec": service_spec_schema(),
            "io.k8s.api.core.v1.ConfigMap": configmap_schema(),
            "io.k8s.api.core.v1.Secret": secret_schema(),
            "io.k8s.api.core.v1.Endpoints": endpoints_schema(),

            // Apps v1 types
            "io.k8s.api.apps.v1.Deployment": deployment_schema(),
            "io.k8s.api.apps.v1.DeploymentSpec": deployment_spec_schema(),
            "io.k8s.api.apps.v1.ReplicaSet": replicaset_schema(),
            "io.k8s.api.apps.v1.ReplicaSetSpec": replicaset_spec_schema(),
            "io.k8s.api.apps.v1.DaemonSet": daemonset_schema(),
            "io.k8s.api.apps.v1.StatefulSet": statefulset_schema(),

            // Batch v1 types
            "io.k8s.api.batch.v1.Job": job_schema(),
            "io.k8s.api.batch.v1.CronJob": cronjob_schema(),

            // Storage types
            "io.k8s.api.core.v1.PersistentVolume": pv_schema(),
            "io.k8s.api.core.v1.PersistentVolumeClaim": pvc_schema(),
            "io.k8s.api.storage.v1.StorageClass": storageclass_schema(),

            // RBAC types
            "io.k8s.api.rbac.v1.Role": role_schema(),
            "io.k8s.api.rbac.v1.RoleBinding": rolebinding_schema(),
            "io.k8s.api.rbac.v1.ClusterRole": clusterrole_schema(),
            "io.k8s.api.rbac.v1.ClusterRoleBinding": clusterrolebinding_schema(),

            // ServiceAccount
            "io.k8s.api.core.v1.ServiceAccount": serviceaccount_schema(),

            // Event
            "io.k8s.api.core.v1.Event": event_schema(),

            // Common types
            "io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta": object_meta_schema(),
            "io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector": label_selector_schema(),
            "io.k8s.api.core.v1.PodTemplateSpec": pod_template_spec_schema(),
        }
    });

    // Return JSON response
    let schema_json = serde_json::to_string(&schema).unwrap();
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(axum::body::Body::from(schema_json))
        .unwrap()
}

/// Generate OpenAPI v3 schema
pub async fn openapi_v3() -> impl IntoResponse {
    // Return empty paths to indicate v3 is not fully supported
    // This forces kubectl to fall back to OpenAPI v2
    let discovery = json!({
        "paths": {}
    });

    Json(discovery)
}

fn object_meta_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "namespace": {"type": "string"},
            "uid": {"type": "string"},
            "resourceVersion": {"type": "string"},
            "creationTimestamp": {"type": "string", "format": "date-time"},
            "labels": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "annotations": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            }
        }
    })
}

fn label_selector_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "matchLabels": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            }
        }
    })
}

fn pod_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Pod",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"$ref": "#/definitions/io.k8s.api.core.v1.PodSpec"},
            "status": {"$ref": "#/definitions/io.k8s.api.core.v1.PodStatus"}
        }
    })
}

fn pod_spec_schema() -> Value {
    json!({
        "type": "object",
        "required": ["containers"],
        "properties": {
            "containers": {
                "type": "array",
                "items": {"$ref": "#/definitions/io.k8s.api.core.v1.Container"}
            },
            "restartPolicy": {"type": "string", "enum": ["Always", "OnFailure", "Never"]},
            "nodeName": {"type": "string"},
            "nodeSelector": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "serviceAccountName": {"type": "string"},
            "hostNetwork": {"type": "boolean"},
            "dnsPolicy": {"type": "string"}
        }
    })
}

fn pod_status_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "phase": {"type": "string"},
            "hostIP": {"type": "string"},
            "podIP": {"type": "string"},
            "startTime": {"type": "string", "format": "date-time"}
        }
    })
}

fn container_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name", "image"],
        "properties": {
            "name": {"type": "string"},
            "image": {"type": "string"},
            "command": {"type": "array", "items": {"type": "string"}},
            "args": {"type": "array", "items": {"type": "string"}},
            "env": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "value": {"type": "string"}
                    }
                }
            },
            "ports": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "containerPort": {"type": "integer"},
                        "protocol": {"type": "string"}
                    }
                }
            },
            "resources": {"type": "object"},
            "volumeMounts": {"type": "array"},
            "imagePullPolicy": {"type": "string"}
        }
    })
}

fn namespace_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Namespace",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"type": "object"},
            "status": {"type": "object"}
        }
    })
}

fn node_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Node",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"type": "object"},
            "status": {"type": "object"}
        }
    })
}

fn service_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Service",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"$ref": "#/definitions/io.k8s.api.core.v1.ServiceSpec"},
            "status": {"type": "object"}
        }
    })
}

fn service_spec_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "type": {"type": "string", "enum": ["ClusterIP", "NodePort", "LoadBalancer", "ExternalName"]},
            "selector": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "ports": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "port": {"type": "integer"},
                        "targetPort": {"type": "integer"},
                        "protocol": {"type": "string"},
                        "nodePort": {"type": "integer"}
                    }
                }
            },
            "clusterIP": {"type": "string"},
            "externalIPs": {"type": "array", "items": {"type": "string"}}
        }
    })
}

fn configmap_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "ConfigMap",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "data": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "binaryData": {
                "type": "object",
                "additionalProperties": {"type": "string", "format": "byte"}
            }
        }
    })
}

fn secret_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Secret",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "type": {"type": "string"},
            "data": {
                "type": "object",
                "additionalProperties": {"type": "string", "format": "byte"}
            },
            "stringData": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            }
        }
    })
}

fn endpoints_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Endpoints",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "subsets": {"type": "array"}
        }
    })
}

fn deployment_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "apps",
            "kind": "Deployment",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"$ref": "#/definitions/io.k8s.api.apps.v1.DeploymentSpec"},
            "status": {"type": "object"}
        }
    })
}

fn deployment_spec_schema() -> Value {
    json!({
        "type": "object",
        "required": ["selector", "template"],
        "properties": {
            "replicas": {"type": "integer", "minimum": 0},
            "selector": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector"},
            "template": {"$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec"},
            "strategy": {"type": "object"},
            "minReadySeconds": {"type": "integer"},
            "revisionHistoryLimit": {"type": "integer"}
        }
    })
}

fn pod_template_spec_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"$ref": "#/definitions/io.k8s.api.core.v1.PodSpec"}
        }
    })
}

fn replicaset_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "apps",
            "kind": "ReplicaSet",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"$ref": "#/definitions/io.k8s.api.apps.v1.ReplicaSetSpec"},
            "status": {"type": "object"}
        }
    })
}

fn replicaset_spec_schema() -> Value {
    json!({
        "type": "object",
        "required": ["selector", "template"],
        "properties": {
            "replicas": {"type": "integer", "minimum": 0},
            "selector": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector"},
            "template": {"$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec"},
            "minReadySeconds": {"type": "integer"}
        }
    })
}

fn daemonset_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "apps",
            "kind": "DaemonSet",
            "version": "v1"
        }],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {"type": "object"},
            "status": {"type": "object"}
        }
    })
}

// === Additional resource schemas ===

fn statefulset_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "apps",
            "kind": "StatefulSet",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {
                "type": "object",
                "required": ["serviceName", "selector", "template"],
                "properties": {
                    "serviceName": {"type": "string"},
                    "replicas": {"type": "integer", "minimum": 0},
                    "selector": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.LabelSelector"},
                    "template": {"$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec"},
                    "volumeClaimTemplates": {"type": "array"},
                    "podManagementPolicy": {"type": "string", "enum": ["OrderedReady", "Parallel"]}
                }
            },
            "status": {"type": "object"}
        }
    })
}

pub fn job_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "batch",
            "kind": "Job",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {
                "type": "object",
                "required": ["template"],
                "properties": {
                    "parallelism": {"type": "integer", "minimum": 0},
                    "completions": {"type": "integer", "minimum": 0},
                    "backoffLimit": {"type": "integer", "minimum": 0},
                    "template": {"$ref": "#/definitions/io.k8s.api.core.v1.PodTemplateSpec"},
                    "ttlSecondsAfterFinished": {"type": "integer"},
                    "suspend": {"type": "boolean"}
                }
            },
            "status": {"type": "object"}
        }
    })
}

pub fn cronjob_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "batch",
            "kind": "CronJob",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {
                "type": "object",
                "required": ["schedule", "jobTemplate"],
                "properties": {
                    "schedule": {"type": "string"},
                    "suspend": {"type": "boolean"},
                    "concurrencyPolicy": {"type": "string", "enum": ["Allow", "Forbid", "Replace"]},
                    "successfulJobsHistoryLimit": {"type": "integer"},
                    "failedJobsHistoryLimit": {"type": "integer"},
                    "jobTemplate": {"type": "object"}
                }
            },
            "status": {"type": "object"}
        }
    })
}

pub fn pv_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "PersistentVolume",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {
                "type": "object",
                "properties": {
                    "capacity": {
                        "type": "object",
                        "additionalProperties": {"type": "string"}
                    },
                    "accessModes": {
                        "type": "array",
                        "items": {"type": "string", "enum": ["ReadWriteOnce", "ReadOnlyMany", "ReadWriteMany"]}
                    },
                    "persistentVolumeReclaimPolicy": {"type": "string", "enum": ["Retain", "Recycle", "Delete"]},
                    "storageClassName": {"type": "string"}
                }
            },
            "status": {"type": "object"}
        }
    })
}

pub fn pvc_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "PersistentVolumeClaim",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "spec": {
                "type": "object",
                "properties": {
                    "accessModes": {
                        "type": "array",
                        "items": {"type": "string"}
                    },
                    "resources": {
                        "type": "object",
                        "properties": {
                            "requests": {
                                "type": "object",
                                "additionalProperties": {"type": "string"}
                            }
                        }
                    },
                    "storageClassName": {"type": "string"},
                    "volumeName": {"type": "string"}
                }
            },
            "status": {"type": "object"}
        }
    })
}

pub fn storageclass_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "storage.k8s.io",
            "kind": "StorageClass",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind", "provisioner"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "provisioner": {"type": "string"},
            "parameters": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "reclaimPolicy": {"type": "string"},
            "volumeBindingMode": {"type": "string"}
        }
    })
}

pub fn role_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "rbac.authorization.k8s.io",
            "kind": "Role",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "rules": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "apiGroups": {"type": "array", "items": {"type": "string"}},
                        "resources": {"type": "array", "items": {"type": "string"}},
                        "verbs": {"type": "array", "items": {"type": "string"}},
                        "resourceNames": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

pub fn rolebinding_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "rbac.authorization.k8s.io",
            "kind": "RoleBinding",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind", "roleRef"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "subjects": {
                "type": "array",
                "items": {"type": "object"}
            },
            "roleRef": {
                "type": "object",
                "required": ["apiGroup", "kind", "name"],
                "properties": {
                    "apiGroup": {"type": "string"},
                    "kind": {"type": "string"},
                    "name": {"type": "string"}
                }
            }
        }
    })
}

pub fn clusterrole_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "rbac.authorization.k8s.io",
            "kind": "ClusterRole",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "rules": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "apiGroups": {"type": "array", "items": {"type": "string"}},
                        "resources": {"type": "array", "items": {"type": "string"}},
                        "verbs": {"type": "array", "items": {"type": "string"}},
                        "resourceNames": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

pub fn clusterrolebinding_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "rbac.authorization.k8s.io",
            "kind": "ClusterRoleBinding",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind", "roleRef"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "subjects": {
                "type": "array",
                "items": {"type": "object"}
            },
            "roleRef": {
                "type": "object",
                "required": ["apiGroup", "kind", "name"],
                "properties": {
                    "apiGroup": {"type": "string"},
                    "kind": {"type": "string"},
                    "name": {"type": "string"}
                }
            }
        }
    })
}

pub fn serviceaccount_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "ServiceAccount",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "secrets": {
                "type": "array",
                "items": {"type": "object"}
            },
            "imagePullSecrets": {
                "type": "array",
                "items": {"type": "object"}
            },
            "automountServiceAccountToken": {"type": "boolean"}
        }
    })
}

pub fn event_schema() -> Value {
    json!({
        "type": "object",
        "x-kubernetes-group-version-kind": [{
            "group": "",
            "kind": "Event",
            "version": "v1"
        }],
        "required": ["apiVersion", "kind"],
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {"$ref": "#/definitions/io.k8s.apimachinery.pkg.apis.meta.v1.ObjectMeta"},
            "involvedObject": {"type": "object"},
            "reason": {"type": "string"},
            "message": {"type": "string"},
            "type": {"type": "string"},
            "count": {"type": "integer"},
            "firstTimestamp": {"type": "string", "format": "date-time"},
            "lastTimestamp": {"type": "string", "format": "date-time"}
        }
    })
}
