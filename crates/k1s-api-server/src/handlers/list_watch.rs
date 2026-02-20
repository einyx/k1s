//! Combined list/watch handlers that support both JSON and SSE responses

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::stream::{self};
use std::convert::Infallible;

use k1s_storage::backend::ResourceStore;
use k1s_storage::{Storage, WatchEvent, WatchEventType};
use k1s_types::{ParsedFieldSelector, Resource, ResourceList};

use super::generic::ListParams;
use crate::error::ApiResult;
use crate::state::AppState;

/// Convert a WatchEvent to an SSE event
fn watch_event_to_sse<R: Resource>(event: WatchEvent) -> Result<Event, Infallible> {
    let event_type = match event.event_type {
        WatchEventType::Added => "ADDED",
        WatchEventType::Modified => "MODIFIED",
        WatchEventType::Deleted => "DELETED",
    };

    let object: Option<R> = event
        .value
        .and_then(|v| serde_json::from_slice(&v).ok());

    let data = serde_json::json!({
        "type": event_type,
        "object": object,
    });

    Ok(Event::default().data(data.to_string()))
}

/// Unified list/watch handler for namespaced resources
/// Returns SSE stream if watch=true, otherwise returns JSON list
pub async fn list_or_watch_namespaced<R: Resource + 'static>(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(params): Query<ListParams>,
) -> ApiResult<Response> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    if params.watch.unwrap_or(false) {
        // Return SSE stream for watch
        let watcher = store.watch(Some(&namespace)).await?;

        let stream = stream::unfold(watcher, |mut w| async move {
            match w.next().await {
                Some(event) => {
                    let sse_event = watch_event_to_sse::<R>(event);
                    Some((sse_event, w))
                }
                None => None,
            }
        });

        Ok(Sse::new(stream)
            .keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(std::time::Duration::from_secs(30))
            )
            .into_response())
    } else {
        // Return JSON list
        let label_selector = params.parse_label_selector();
        let field_selector = params.parse_field_selector();
        let mut resources = store.list_with_selector(Some(&namespace), label_selector.as_ref()).await?;

        // Apply field selector
        if let Some(fs) = &field_selector {
            resources = filter_by_field_selector(resources, fs);
        }

        let revision = state.storage.revision().await?;
        let list = ResourceList::new(resources).with_resource_version(&revision.to_string());
        Ok(Json(list).into_response())
    }
}

/// Unified list/watch handler for cluster-scoped resources
pub async fn list_or_watch_cluster<R: Resource + 'static>(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> ApiResult<Response> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    if params.watch.unwrap_or(false) {
        let watcher = store.watch(None).await?;

        let stream = stream::unfold(watcher, |mut w| async move {
            match w.next().await {
                Some(event) => {
                    let sse_event = watch_event_to_sse::<R>(event);
                    Some((sse_event, w))
                }
                None => None,
            }
        });

        Ok(Sse::new(stream)
            .keep_alive(
                axum::response::sse::KeepAlive::new()
                    .interval(std::time::Duration::from_secs(30))
            )
            .into_response())
    } else {
        let label_selector = params.parse_label_selector();
        let field_selector = params.parse_field_selector();
        let mut resources = store.list_with_selector(None, label_selector.as_ref()).await?;

        // Apply field selector
        if let Some(fs) = &field_selector {
            resources = filter_by_field_selector(resources, fs);
        }

        let revision = state.storage.revision().await?;
        let list = ResourceList::new(resources).with_resource_version(&revision.to_string());
        Ok(Json(list).into_response())
    }
}

/// Filter resources by field selector
fn filter_by_field_selector<R: Resource>(
    resources: Vec<R>,
    selector: &ParsedFieldSelector,
) -> Vec<R> {
    resources
        .into_iter()
        .filter(|r| {
            // Convert to JSON to apply field selector
            if let Ok(json) = serde_json::to_value(r) {
                selector.matches_json(&json)
            } else {
                false
            }
        })
        .collect()
}

/// Unified list/watch handler for all namespaces
pub async fn list_or_watch_all<R: Resource + 'static>(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> ApiResult<Response> {
    // Same as cluster-scoped for listing all
    list_or_watch_cluster::<R>(State(state), Query(params)).await
}

// Type-specific wrapper functions for routing

use k1s_types::{
    Pod, Namespace, Node, Service, ConfigMap, Secret, Endpoints, Deployment, ReplicaSet,
    apps_v1::DaemonSet,
    batch_v1::{Job, CronJob},
};

// Pod handlers
pub async fn list_pods(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Pod>(state, path, query).await
}

pub async fn list_all_pods(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<Pod>(state, query).await
}

// Namespace handlers
pub async fn list_namespaces(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_cluster::<Namespace>(state, query).await
}

// Node handlers
pub async fn list_nodes(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_cluster::<Node>(state, query).await
}

// Service handlers
pub async fn list_services(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Service>(state, path, query).await
}

// ConfigMap handlers
pub async fn list_configmaps(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<ConfigMap>(state, path, query).await
}

// Secret handlers
pub async fn list_secrets(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Secret>(state, path, query).await
}

// Endpoints handlers
pub async fn list_endpoints(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Endpoints>(state, path, query).await
}

pub async fn list_all_endpoints(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<Endpoints>(state, query).await
}

// Deployment handlers
pub async fn list_deployments(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Deployment>(state, path, query).await
}

pub async fn list_all_deployments(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<Deployment>(state, query).await
}

// ReplicaSet handlers
pub async fn list_replicasets(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<ReplicaSet>(state, path, query).await
}

pub async fn list_all_replicasets(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<ReplicaSet>(state, query).await
}

// DaemonSet handlers
pub async fn list_daemonsets(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<DaemonSet>(state, path, query).await
}

pub async fn list_all_daemonsets(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<DaemonSet>(state, query).await
}

// Job handlers
pub async fn list_jobs(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<Job>(state, path, query).await
}

pub async fn list_all_jobs(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<Job>(state, query).await
}

// CronJob handlers
pub async fn list_cronjobs(
    state: State<AppState>,
    path: Path<String>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_namespaced::<CronJob>(state, path, query).await
}

pub async fn list_all_cronjobs(
    state: State<AppState>,
    query: Query<ListParams>,
) -> ApiResult<Response> {
    list_or_watch_all::<CronJob>(state, query).await
}
