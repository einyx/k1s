//! Event API handlers

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{Event, ResourceList};

use super::generic::ListParams;
use super::patch::patch_namespaced;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// List events in a namespace
pub async fn list_events(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Event>>> {
    let store = ResourceStore::<Event>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

/// List events across all namespaces
pub async fn list_all_events(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Event>>> {
    let store = ResourceStore::<Event>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

/// Get a specific event
pub async fn get_event(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Event>> {
    let store = ResourceStore::<Event>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(event) => Ok(Json(event)),
        None => Err(ApiError::NotFound {
            kind: "Event".to_string(),
            name,
        }),
    }
}

/// Create an event
pub async fn create_event(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut event): Json<Event>,
) -> ApiResult<impl IntoResponse> {
    event.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Event>::new(state.storage.clone());
    let created = store.create(event).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

/// Update an event
pub async fn update_event(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut event): Json<Event>,
) -> ApiResult<Json<Event>> {
    event.metadata.namespace = Some(namespace);
    event.metadata.name = name;
    let store = ResourceStore::<Event>::new(state.storage.clone());
    let updated = store.update(event).await?;
    Ok(Json(updated))
}

/// Delete an event
pub async fn delete_event(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Event>> {
    let store = ResourceStore::<Event>::new(state.storage.clone());
    let event = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match event {
            Some(e) => Ok(Json(e)),
            None => Err(ApiError::NotFound {
                kind: "Event".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Event".to_string(),
            name,
        })
    }
}

/// Patch an event
pub async fn patch_event(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<Event>> {
    patch_namespaced::<Event>(state, path, headers, body).await
}
