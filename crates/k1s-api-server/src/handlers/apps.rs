//! Apps v1 API handlers (Deployments, ReplicaSets, DaemonSets)

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{Deployment, ReplicaSet, ResourceList};

use super::generic::ListParams;
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
