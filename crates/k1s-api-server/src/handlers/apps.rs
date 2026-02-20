//! Apps v1 API handlers (Deployments, ReplicaSets, DaemonSets)

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{apps_v1::{DaemonSet, StatefulSet}, Deployment, ReplicaSet, ResourceList};

use super::generic::ListParams;
use super::patch::patch_namespaced;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// ReplicaSet handlers

pub async fn list_replicasets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ReplicaSet>>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_replicasets(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ReplicaSet>>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ReplicaSet>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(rs) => Ok(Json(rs)),
        None => Err(ApiError::NotFound {
            kind: "ReplicaSet".to_string(),
            name,
        }),
    }
}

pub async fn create_replicaset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut rs): Json<ReplicaSet>,
) -> ApiResult<impl IntoResponse> {
    rs.metadata.namespace = Some(namespace);
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    let created = store.create(rs).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut rs): Json<ReplicaSet>,
) -> ApiResult<Json<ReplicaSet>> {
    rs.metadata.namespace = Some(namespace);
    rs.metadata.name = name;
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    let updated = store.update(rs).await?;
    Ok(Json(updated))
}

pub async fn delete_replicaset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ReplicaSet>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());
    let rs = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match rs {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: "ReplicaSet".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "ReplicaSet".to_string(),
            name,
        })
    }
}

pub async fn patch_replicaset(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<ReplicaSet>> {
    patch_namespaced::<ReplicaSet>(state, path, headers, body).await
}

// Deployment handlers

pub async fn list_deployments(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Deployment>>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_deployments(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Deployment>>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Deployment>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(d) => Ok(Json(d)),
        None => Err(ApiError::NotFound {
            kind: "Deployment".to_string(),
            name,
        }),
    }
}

pub async fn create_deployment(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut deployment): Json<Deployment>,
) -> ApiResult<impl IntoResponse> {
    deployment.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    let created = store.create(deployment).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut deployment): Json<Deployment>,
) -> ApiResult<Json<Deployment>> {
    deployment.metadata.namespace = Some(namespace);
    deployment.metadata.name = name;
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    let updated = store.update(deployment).await?;
    Ok(Json(updated))
}

pub async fn delete_deployment(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Deployment>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());
    let deployment = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match deployment {
            Some(d) => Ok(Json(d)),
            None => Err(ApiError::NotFound {
                kind: "Deployment".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Deployment".to_string(),
            name,
        })
    }
}

pub async fn patch_deployment(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<Deployment>> {
    patch_namespaced::<Deployment>(state, path, headers, body).await
}

// DaemonSet handlers

pub async fn list_daemonsets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<DaemonSet>>> {
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_daemonsets(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<DaemonSet>>> {
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<DaemonSet>> {
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(ds) => Ok(Json(ds)),
        None => Err(ApiError::NotFound {
            kind: "DaemonSet".to_string(),
            name,
        }),
    }
}

pub async fn create_daemonset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut ds): Json<DaemonSet>,
) -> ApiResult<impl IntoResponse> {
    ds.metadata.namespace = Some(namespace);
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    let created = store.create(ds).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut ds): Json<DaemonSet>,
) -> ApiResult<Json<DaemonSet>> {
    ds.metadata.namespace = Some(namespace);
    ds.metadata.name = name;
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    let updated = store.update(ds).await?;
    Ok(Json(updated))
}

pub async fn delete_daemonset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<DaemonSet>> {
    let store = ResourceStore::<DaemonSet>::new(state.storage.clone());
    let ds = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match ds {
            Some(d) => Ok(Json(d)),
            None => Err(ApiError::NotFound {
                kind: "DaemonSet".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "DaemonSet".to_string(),
            name,
        })
    }
}

pub async fn patch_daemonset(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<DaemonSet>> {
    patch_namespaced::<DaemonSet>(state, path, headers, body).await
}

// StatefulSet handlers

pub async fn list_statefulsets(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<StatefulSet>>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_statefulsets(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<StatefulSet>>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<StatefulSet>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(sts) => Ok(Json(sts)),
        None => Err(ApiError::NotFound {
            kind: "StatefulSet".to_string(),
            name,
        }),
    }
}

pub async fn create_statefulset(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut sts): Json<StatefulSet>,
) -> ApiResult<impl IntoResponse> {
    sts.metadata.namespace = Some(namespace);
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    let created = store.create(sts).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut sts): Json<StatefulSet>,
) -> ApiResult<Json<StatefulSet>> {
    sts.metadata.namespace = Some(namespace);
    sts.metadata.name = name;
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    let updated = store.update(sts).await?;
    Ok(Json(updated))
}

pub async fn delete_statefulset(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<StatefulSet>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());
    let sts = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match sts {
            Some(s) => Ok(Json(s)),
            None => Err(ApiError::NotFound {
                kind: "StatefulSet".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "StatefulSet".to_string(),
            name,
        })
    }
}

pub async fn patch_statefulset(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<StatefulSet>> {
    patch_namespaced::<StatefulSet>(state, path, headers, body).await
}
