//! Batch v1 API handlers (Jobs, CronJobs)

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{batch_v1::{CronJob, Job}, ResourceList};

use super::generic::ListParams;
use super::patch::patch_namespaced;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// Job handlers

pub async fn list_jobs(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Job>>> {
    let store = ResourceStore::<Job>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_jobs(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Job>>> {
    let store = ResourceStore::<Job>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_job(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Job>> {
    let store = ResourceStore::<Job>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(job) => Ok(Json(job)),
        None => Err(ApiError::NotFound {
            kind: "Job".to_string(),
            name,
        }),
    }
}

pub async fn create_job(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut job): Json<Job>,
) -> ApiResult<impl IntoResponse> {
    job.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Job>::new(state.storage.clone());
    let created = store.create(job).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_job(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut job): Json<Job>,
) -> ApiResult<Json<Job>> {
    job.metadata.namespace = Some(namespace);
    job.metadata.name = name;
    let store = ResourceStore::<Job>::new(state.storage.clone());
    let updated = store.update(job).await?;
    Ok(Json(updated))
}

pub async fn delete_job(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Job>> {
    let store = ResourceStore::<Job>::new(state.storage.clone());
    let job = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match job {
            Some(j) => Ok(Json(j)),
            None => Err(ApiError::NotFound {
                kind: "Job".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Job".to_string(),
            name,
        })
    }
}

pub async fn patch_job(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<Job>> {
    patch_namespaced::<Job>(state, path, headers, body).await
}

// CronJob handlers

pub async fn list_cronjobs(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<CronJob>>> {
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_cronjobs(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<CronJob>>> {
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_cronjob(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<CronJob>> {
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(cj) => Ok(Json(cj)),
        None => Err(ApiError::NotFound {
            kind: "CronJob".to_string(),
            name,
        }),
    }
}

pub async fn create_cronjob(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut cj): Json<CronJob>,
) -> ApiResult<impl IntoResponse> {
    cj.metadata.namespace = Some(namespace);
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    let created = store.create(cj).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_cronjob(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut cj): Json<CronJob>,
) -> ApiResult<Json<CronJob>> {
    cj.metadata.namespace = Some(namespace);
    cj.metadata.name = name;
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    let updated = store.update(cj).await?;
    Ok(Json(updated))
}

pub async fn delete_cronjob(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<CronJob>> {
    let store = ResourceStore::<CronJob>::new(state.storage.clone());
    let cj = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match cj {
            Some(c) => Ok(Json(c)),
            None => Err(ApiError::NotFound {
                kind: "CronJob".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "CronJob".to_string(),
            name,
        })
    }
}

pub async fn patch_cronjob(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<CronJob>> {
    patch_namespaced::<CronJob>(state, path, headers, body).await
}
