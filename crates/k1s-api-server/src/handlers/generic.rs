//! Generic resource handlers

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{Resource, ResourceList};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for list operations
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    pub label_selector: Option<String>,
    pub field_selector: Option<String>,
    pub watch: Option<bool>,
    pub resource_version: Option<String>,
    pub limit: Option<i64>,
    #[serde(rename = "continue")]
    pub continue_: Option<String>,
}

/// Generic list handler for namespaced resources
pub async fn list_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<R>>> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let resources = store.list(Some(&namespace)).await?;

    let revision = state.storage.revision().await?;
    let list = ResourceList::new(resources).with_resource_version(&revision.to_string());

    Ok(Json(list))
}

/// Generic list handler for cluster-scoped resources
pub async fn list_cluster<R: Resource>(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<R>>> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let resources = store.list(None).await?;

    let revision = state.storage.revision().await?;
    let list = ResourceList::new(resources).with_resource_version(&revision.to_string());

    Ok(Json(list))
}

/// Generic list across all namespaces
pub async fn list_all_namespaces<R: Resource>(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<R>>> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let resources = store.list(None).await?;

    let revision = state.storage.revision().await?;
    let list = ResourceList::new(resources).with_resource_version(&revision.to_string());

    Ok(Json(list))
}

/// Generic get handler for namespaced resources
pub async fn get_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<R>> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(resource) => Ok(Json(resource)),
        None => Err(ApiError::NotFound {
            kind: R::KIND.to_string(),
            name,
        }),
    }
}

/// Generic get handler for cluster-scoped resources
pub async fn get_cluster<R: Resource>(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<R>> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    match store.get(None, &name).await? {
        Some(resource) => Ok(Json(resource)),
        None => Err(ApiError::NotFound {
            kind: R::KIND.to_string(),
            name,
        }),
    }
}

/// Generic create handler for namespaced resources
pub async fn create_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut resource): Json<R>,
) -> ApiResult<impl IntoResponse> {
    // Ensure namespace matches
    let meta = resource.metadata_mut();
    meta.namespace = Some(namespace);

    let store = ResourceStore::<R>::new(state.storage.clone());
    let created = store.create(resource).await?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// Generic create handler for cluster-scoped resources
pub async fn create_cluster<R: Resource>(
    State(state): State<AppState>,
    Json(resource): Json<R>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let created = store.create(resource).await?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// Generic update handler for namespaced resources
pub async fn update_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut resource): Json<R>,
) -> ApiResult<Json<R>> {
    // Ensure namespace and name match
    let meta = resource.metadata_mut();
    meta.namespace = Some(namespace);
    meta.name = name;

    let store = ResourceStore::<R>::new(state.storage.clone());
    let updated = store.update(resource).await?;

    Ok(Json(updated))
}

/// Generic update handler for cluster-scoped resources
pub async fn update_cluster<R: Resource>(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut resource): Json<R>,
) -> ApiResult<Json<R>> {
    resource.metadata_mut().name = name;

    let store = ResourceStore::<R>::new(state.storage.clone());
    let updated = store.update(resource).await?;

    Ok(Json(updated))
}

/// Generic delete handler for namespaced resources
pub async fn delete_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    // Get the resource first for the response
    let resource = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match resource {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: R::KIND.to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: R::KIND.to_string(),
            name,
        })
    }
}

/// Generic delete handler for cluster-scoped resources
pub async fn delete_cluster<R: Resource>(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    let resource = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match resource {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: R::KIND.to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: R::KIND.to_string(),
            name,
        })
    }
}
