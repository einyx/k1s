//! Core v1 API handlers

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{
    ConfigMap, Endpoints, Namespace, NamespacePhase, NamespaceStatus, Node, Pod, ResourceList,
    Secret, Service,
};

use super::generic::ListParams;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// Namespace handlers (cluster-scoped)

pub async fn list_namespaces(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Namespace>>> {
    let store = ResourceStore::<Namespace>::new(state.storage.clone());
    let namespaces = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(namespaces).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<Namespace>> {
    let store = ResourceStore::<Namespace>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(ns) => Ok(Json(ns)),
        None => Err(ApiError::NotFound {
            kind: "Namespace".to_string(),
            name,
        }),
    }
}

pub async fn create_namespace(
    State(state): State<AppState>,
    Json(mut namespace): Json<Namespace>,
) -> ApiResult<impl IntoResponse> {
    // Set default status
    if namespace.status.is_none() {
        namespace.status = Some(NamespaceStatus {
            phase: Some(NamespacePhase::Active),
            conditions: vec![],
        });
    }

    let store = ResourceStore::<Namespace>::new(state.storage.clone());
    let created = store.create(namespace).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn delete_namespace(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<Namespace>> {
    let store = ResourceStore::<Namespace>::new(state.storage.clone());
    let ns = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match ns {
            Some(n) => Ok(Json(n)),
            None => Err(ApiError::NotFound {
                kind: "Namespace".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Namespace".to_string(),
            name,
        })
    }
}

// Node handlers (cluster-scoped)

pub async fn list_nodes(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Node>>> {
    let store = ResourceStore::<Node>::new(state.storage.clone());
    let nodes = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(nodes).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<Node>> {
    let store = ResourceStore::<Node>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(node) => Ok(Json(node)),
        None => Err(ApiError::NotFound {
            kind: "Node".to_string(),
            name,
        }),
    }
}

pub async fn create_node(
    State(state): State<AppState>,
    Json(node): Json<Node>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<Node>::new(state.storage.clone());
    let created = store.create(node).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut node): Json<Node>,
) -> ApiResult<Json<Node>> {
    node.metadata.name = name;
    let store = ResourceStore::<Node>::new(state.storage.clone());
    let updated = store.update(node).await?;
    Ok(Json(updated))
}

pub async fn delete_node(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<Node>> {
    let store = ResourceStore::<Node>::new(state.storage.clone());
    let node = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match node {
            Some(n) => Ok(Json(n)),
            None => Err(ApiError::NotFound {
                kind: "Node".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Node".to_string(),
            name,
        })
    }
}

// Pod handlers (namespaced)

pub async fn list_pods(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Pod>>> {
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    let pods = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(pods).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_pods(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Pod>>> {
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    let pods = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(pods).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Pod>> {
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(pod) => Ok(Json(pod)),
        None => Err(ApiError::NotFound {
            kind: "Pod".to_string(),
            name,
        }),
    }
}

pub async fn create_pod(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut pod): Json<Pod>,
) -> ApiResult<impl IntoResponse> {
    pod.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    let created = store.create(pod).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut pod): Json<Pod>,
) -> ApiResult<Json<Pod>> {
    pod.metadata.namespace = Some(namespace);
    pod.metadata.name = name;
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    let updated = store.update(pod).await?;
    Ok(Json(updated))
}

pub async fn delete_pod(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Pod>> {
    let store = ResourceStore::<Pod>::new(state.storage.clone());
    let pod = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match pod {
            Some(p) => Ok(Json(p)),
            None => Err(ApiError::NotFound {
                kind: "Pod".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Pod".to_string(),
            name,
        })
    }
}

// Service handlers (namespaced)

pub async fn list_services(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Service>>> {
    let store = ResourceStore::<Service>::new(state.storage.clone());
    let services = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(services).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Service>> {
    let store = ResourceStore::<Service>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(svc) => Ok(Json(svc)),
        None => Err(ApiError::NotFound {
            kind: "Service".to_string(),
            name,
        }),
    }
}

pub async fn create_service(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut service): Json<Service>,
) -> ApiResult<impl IntoResponse> {
    service.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Service>::new(state.storage.clone());
    let created = store.create(service).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn delete_service(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Service>> {
    let store = ResourceStore::<Service>::new(state.storage.clone());
    let svc = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match svc {
            Some(s) => Ok(Json(s)),
            None => Err(ApiError::NotFound {
                kind: "Service".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Service".to_string(),
            name,
        })
    }
}

// ConfigMap handlers (namespaced)

pub async fn list_configmaps(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ConfigMap>>> {
    let store = ResourceStore::<ConfigMap>::new(state.storage.clone());
    let cms = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(cms).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ConfigMap>> {
    let store = ResourceStore::<ConfigMap>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(cm) => Ok(Json(cm)),
        None => Err(ApiError::NotFound {
            kind: "ConfigMap".to_string(),
            name,
        }),
    }
}

pub async fn create_configmap(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut cm): Json<ConfigMap>,
) -> ApiResult<impl IntoResponse> {
    cm.metadata.namespace = Some(namespace);
    let store = ResourceStore::<ConfigMap>::new(state.storage.clone());
    let created = store.create(cm).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut cm): Json<ConfigMap>,
) -> ApiResult<Json<ConfigMap>> {
    cm.metadata.namespace = Some(namespace);
    cm.metadata.name = name;
    let store = ResourceStore::<ConfigMap>::new(state.storage.clone());
    let updated = store.update(cm).await?;
    Ok(Json(updated))
}

pub async fn delete_configmap(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ConfigMap>> {
    let store = ResourceStore::<ConfigMap>::new(state.storage.clone());
    let cm = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match cm {
            Some(c) => Ok(Json(c)),
            None => Err(ApiError::NotFound {
                kind: "ConfigMap".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "ConfigMap".to_string(),
            name,
        })
    }
}

// Secret handlers (namespaced)

pub async fn list_secrets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Secret>>> {
    let store = ResourceStore::<Secret>::new(state.storage.clone());
    let secrets = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(secrets).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Secret>> {
    let store = ResourceStore::<Secret>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(secret) => Ok(Json(secret)),
        None => Err(ApiError::NotFound {
            kind: "Secret".to_string(),
            name,
        }),
    }
}

pub async fn create_secret(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut secret): Json<Secret>,
) -> ApiResult<impl IntoResponse> {
    secret.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Secret>::new(state.storage.clone());
    let created = store.create(secret).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut secret): Json<Secret>,
) -> ApiResult<Json<Secret>> {
    secret.metadata.namespace = Some(namespace);
    secret.metadata.name = name;
    let store = ResourceStore::<Secret>::new(state.storage.clone());
    let updated = store.update(secret).await?;
    Ok(Json(updated))
}

pub async fn delete_secret(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Secret>> {
    let store = ResourceStore::<Secret>::new(state.storage.clone());
    let secret = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match secret {
            Some(s) => Ok(Json(s)),
            None => Err(ApiError::NotFound {
                kind: "Secret".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Secret".to_string(),
            name,
        })
    }
}

// Endpoints handlers (namespaced)

pub async fn list_endpoints(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Endpoints>>> {
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    let endpoints = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(endpoints).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_endpoints(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Endpoints>>> {
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    let endpoints = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(endpoints).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Endpoints>> {
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(ep) => Ok(Json(ep)),
        None => Err(ApiError::NotFound {
            kind: "Endpoints".to_string(),
            name,
        }),
    }
}

pub async fn create_endpoints(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut endpoints): Json<Endpoints>,
) -> ApiResult<impl IntoResponse> {
    endpoints.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    let created = store.create(endpoints).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut endpoints): Json<Endpoints>,
) -> ApiResult<Json<Endpoints>> {
    endpoints.metadata.namespace = Some(namespace);
    endpoints.metadata.name = name;
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    let updated = store.update(endpoints).await?;
    Ok(Json(updated))
}

pub async fn delete_endpoints(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Endpoints>> {
    let store = ResourceStore::<Endpoints>::new(state.storage.clone());
    let ep = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match ep {
            Some(e) => Ok(Json(e)),
            None => Err(ApiError::NotFound {
                kind: "Endpoints".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Endpoints".to_string(),
            name,
        })
    }
}
