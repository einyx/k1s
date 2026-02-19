//! Watch handler for streaming resource updates

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use futures::stream::{self, Stream};
use std::convert::Infallible;
use tokio_stream::StreamExt;

use k1s_storage::backend::ResourceStore;
use k1s_storage::{WatchEvent, WatchEventType};
use k1s_types::Resource;

use super::generic::ListParams;
use crate::error::ApiResult;
use crate::state::AppState;

/// Watch for changes to namespaced resources
pub async fn watch_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let mut watcher = store.watch(Some(&namespace)).await?;

    let stream = stream::unfold(watcher, |mut w| async move {
        match w.next().await {
            Some(event) => {
                let sse_event = watch_event_to_sse::<R>(event);
                Some((sse_event, w))
            }
            None => None,
        }
    });

    Ok(Sse::new(stream))
}

/// Watch for changes to cluster-scoped resources
pub async fn watch_cluster<R: Resource>(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<R>::new(state.storage.clone());
    let mut watcher = store.watch(None).await?;

    let stream = stream::unfold(watcher, |mut w| async move {
        match w.next().await {
            Some(event) => {
                let sse_event = watch_event_to_sse::<R>(event);
                Some((sse_event, w))
            }
            None => None,
        }
    });

    Ok(Sse::new(stream))
}

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
