//! Storage v1 API handlers (StorageClasses, PVs, PVCs)

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{
    storage_v1::StorageClass,
    PersistentVolume, PersistentVolumeClaim, ServiceAccount,
    ResourceList,
};

use super::generic::ListParams;
use super::patch::{patch_cluster, patch_namespaced};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// StorageClass handlers

pub async fn list_storageclasses(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<StorageClass>>> {
    let store = ResourceStore::<StorageClass>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<StorageClass>> {
    let store = ResourceStore::<StorageClass>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(sc) => Ok(Json(sc)),
        None => Err(ApiError::NotFound {
            kind: "StorageClass".to_string(),
            name,
        }),
    }
}

pub async fn create_storageclass(
    State(state): State<AppState>,
    Json(sc): Json<StorageClass>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<StorageClass>::new(state.storage.clone());
    let created = store.create(sc).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut sc): Json<StorageClass>,
) -> ApiResult<Json<StorageClass>> {
    sc.metadata.name = name;
    let store = ResourceStore::<StorageClass>::new(state.storage.clone());
    let updated = store.update(sc).await?;
    Ok(Json(updated))
}

pub async fn delete_storageclass(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<StorageClass>> {
    let store = ResourceStore::<StorageClass>::new(state.storage.clone());
    let sc = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match sc {
            Some(s) => Ok(Json(s)),
            None => Err(ApiError::NotFound {
                kind: "StorageClass".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "StorageClass".to_string(),
            name,
        })
    }
}

pub async fn patch_storageclass(
    state: State<AppState>,
    path: Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<StorageClass>> {
    patch_cluster::<StorageClass>(state, path, headers, body).await
}

// PersistentVolume handlers

pub async fn list_persistentvolumes(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<PersistentVolume>>> {
    let store = ResourceStore::<PersistentVolume>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_persistentvolume(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<PersistentVolume>> {
    let store = ResourceStore::<PersistentVolume>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(pv) => Ok(Json(pv)),
        None => Err(ApiError::NotFound {
            kind: "PersistentVolume".to_string(),
            name,
        }),
    }
}

pub async fn create_persistentvolume(
    State(state): State<AppState>,
    Json(pv): Json<PersistentVolume>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<PersistentVolume>::new(state.storage.clone());
    let created = store.create(pv).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_persistentvolume(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut pv): Json<PersistentVolume>,
) -> ApiResult<Json<PersistentVolume>> {
    pv.metadata.name = name;
    let store = ResourceStore::<PersistentVolume>::new(state.storage.clone());
    let updated = store.update(pv).await?;
    Ok(Json(updated))
}

pub async fn delete_persistentvolume(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<PersistentVolume>> {
    let store = ResourceStore::<PersistentVolume>::new(state.storage.clone());
    let pv = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match pv {
            Some(p) => Ok(Json(p)),
            None => Err(ApiError::NotFound {
                kind: "PersistentVolume".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "PersistentVolume".to_string(),
            name,
        })
    }
}

pub async fn patch_persistentvolume(
    state: State<AppState>,
    path: Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<PersistentVolume>> {
    patch_cluster::<PersistentVolume>(state, path, headers, body).await
}

// PersistentVolumeClaim handlers

pub async fn list_pvcs(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<PersistentVolumeClaim>>> {
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn list_all_pvcs(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<PersistentVolumeClaim>>> {
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<PersistentVolumeClaim>> {
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(pvc) => Ok(Json(pvc)),
        None => Err(ApiError::NotFound {
            kind: "PersistentVolumeClaim".to_string(),
            name,
        }),
    }
}

pub async fn create_pvc(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut pvc): Json<PersistentVolumeClaim>,
) -> ApiResult<impl IntoResponse> {
    pvc.metadata.namespace = Some(namespace);
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    let created = store.create(pvc).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut pvc): Json<PersistentVolumeClaim>,
) -> ApiResult<Json<PersistentVolumeClaim>> {
    pvc.metadata.namespace = Some(namespace);
    pvc.metadata.name = name;
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    let updated = store.update(pvc).await?;
    Ok(Json(updated))
}

pub async fn delete_pvc(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<PersistentVolumeClaim>> {
    let store = ResourceStore::<PersistentVolumeClaim>::new(state.storage.clone());
    let pvc = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match pvc {
            Some(p) => Ok(Json(p)),
            None => Err(ApiError::NotFound {
                kind: "PersistentVolumeClaim".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "PersistentVolumeClaim".to_string(),
            name,
        })
    }
}

pub async fn patch_pvc(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<PersistentVolumeClaim>> {
    patch_namespaced::<PersistentVolumeClaim>(state, path, headers, body).await
}

// ServiceAccount handlers

pub async fn list_serviceaccounts(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ServiceAccount>>> {
    let store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ServiceAccount>> {
    let store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(sa) => Ok(Json(sa)),
        None => Err(ApiError::NotFound {
            kind: "ServiceAccount".to_string(),
            name,
        }),
    }
}

pub async fn create_serviceaccount(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut sa): Json<ServiceAccount>,
) -> ApiResult<impl IntoResponse> {
    sa.metadata.namespace = Some(namespace);
    let store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
    let created = store.create(sa).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut sa): Json<ServiceAccount>,
) -> ApiResult<Json<ServiceAccount>> {
    sa.metadata.namespace = Some(namespace);
    sa.metadata.name = name;
    let store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
    let updated = store.update(sa).await?;
    Ok(Json(updated))
}

pub async fn delete_serviceaccount(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<ServiceAccount>> {
    let store = ResourceStore::<ServiceAccount>::new(state.storage.clone());
    let sa = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match sa {
            Some(s) => Ok(Json(s)),
            None => Err(ApiError::NotFound {
                kind: "ServiceAccount".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "ServiceAccount".to_string(),
            name,
        })
    }
}

pub async fn patch_serviceaccount(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<ServiceAccount>> {
    patch_namespaced::<ServiceAccount>(state, path, headers, body).await
}
