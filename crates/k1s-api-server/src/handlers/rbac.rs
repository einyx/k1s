//! RBAC v1 API handlers (Roles, ClusterRoles, Bindings)

use axum::extract::{Path, Query, State};
use axum::http::header::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use k1s_storage::{ResourceStore, Storage};
use k1s_types::{
    rbac_v1::{ClusterRole, ClusterRoleBinding, Role, RoleBinding},
    ResourceList,
};

use super::generic::ListParams;
use super::patch::{patch_cluster, patch_namespaced};
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

// Role handlers

pub async fn list_roles(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<Role>>> {
    let store = ResourceStore::<Role>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Role>> {
    let store = ResourceStore::<Role>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError::NotFound {
            kind: "Role".to_string(),
            name,
        }),
    }
}

pub async fn create_role(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut role): Json<Role>,
) -> ApiResult<impl IntoResponse> {
    role.metadata.namespace = Some(namespace);
    let store = ResourceStore::<Role>::new(state.storage.clone());
    let created = store.create(role).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut role): Json<Role>,
) -> ApiResult<Json<Role>> {
    role.metadata.namespace = Some(namespace);
    role.metadata.name = name;
    let store = ResourceStore::<Role>::new(state.storage.clone());
    let updated = store.update(role).await?;
    Ok(Json(updated))
}

pub async fn delete_role(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Role>> {
    let store = ResourceStore::<Role>::new(state.storage.clone());
    let role = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match role {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: "Role".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "Role".to_string(),
            name,
        })
    }
}

pub async fn patch_role(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<Role>> {
    patch_namespaced::<Role>(state, path, headers, body).await
}

// ClusterRole handlers

pub async fn list_clusterroles(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ClusterRole>>> {
    let store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<ClusterRole>> {
    let store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(r) => Ok(Json(r)),
        None => Err(ApiError::NotFound {
            kind: "ClusterRole".to_string(),
            name,
        }),
    }
}

pub async fn create_clusterrole(
    State(state): State<AppState>,
    Json(role): Json<ClusterRole>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    let created = store.create(role).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut role): Json<ClusterRole>,
) -> ApiResult<Json<ClusterRole>> {
    role.metadata.name = name;
    let store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    let updated = store.update(role).await?;
    Ok(Json(updated))
}

pub async fn delete_clusterrole(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<ClusterRole>> {
    let store = ResourceStore::<ClusterRole>::new(state.storage.clone());
    let role = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match role {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: "ClusterRole".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "ClusterRole".to_string(),
            name,
        })
    }
}

pub async fn patch_clusterrole(
    state: State<AppState>,
    path: Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<ClusterRole>> {
    patch_cluster::<ClusterRole>(state, path, headers, body).await
}

// RoleBinding handlers

pub async fn list_rolebindings(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<RoleBinding>>> {
    let store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    let items = store.list(Some(&namespace)).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_rolebinding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<RoleBinding>> {
    let store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    match store.get(Some(&namespace), &name).await? {
        Some(rb) => Ok(Json(rb)),
        None => Err(ApiError::NotFound {
            kind: "RoleBinding".to_string(),
            name,
        }),
    }
}

pub async fn create_rolebinding(
    State(state): State<AppState>,
    Path(namespace): Path<String>,
    Json(mut rb): Json<RoleBinding>,
) -> ApiResult<impl IntoResponse> {
    rb.metadata.namespace = Some(namespace);
    let store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    let created = store.create(rb).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_rolebinding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(mut rb): Json<RoleBinding>,
) -> ApiResult<Json<RoleBinding>> {
    rb.metadata.namespace = Some(namespace);
    rb.metadata.name = name;
    let store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    let updated = store.update(rb).await?;
    Ok(Json(updated))
}

pub async fn delete_rolebinding(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<RoleBinding>> {
    let store = ResourceStore::<RoleBinding>::new(state.storage.clone());
    let rb = store.get(Some(&namespace), &name).await?;

    if store.delete(Some(&namespace), &name).await? {
        match rb {
            Some(r) => Ok(Json(r)),
            None => Err(ApiError::NotFound {
                kind: "RoleBinding".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "RoleBinding".to_string(),
            name,
        })
    }
}

pub async fn patch_rolebinding(
    state: State<AppState>,
    path: Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<RoleBinding>> {
    patch_namespaced::<RoleBinding>(state, path, headers, body).await
}

// ClusterRoleBinding handlers

pub async fn list_clusterrolebindings(
    State(state): State<AppState>,
    Query(_params): Query<ListParams>,
) -> ApiResult<Json<ResourceList<ClusterRoleBinding>>> {
    let store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    let items = store.list(None).await?;
    let revision = state.storage.revision().await?;
    Ok(Json(
        ResourceList::new(items).with_resource_version(&revision.to_string()),
    ))
}

pub async fn get_clusterrolebinding(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<ClusterRoleBinding>> {
    let store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    match store.get(None, &name).await? {
        Some(crb) => Ok(Json(crb)),
        None => Err(ApiError::NotFound {
            kind: "ClusterRoleBinding".to_string(),
            name,
        }),
    }
}

pub async fn create_clusterrolebinding(
    State(state): State<AppState>,
    Json(crb): Json<ClusterRoleBinding>,
) -> ApiResult<impl IntoResponse> {
    let store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    let created = store.create(crb).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_clusterrolebinding(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(mut crb): Json<ClusterRoleBinding>,
) -> ApiResult<Json<ClusterRoleBinding>> {
    crb.metadata.name = name;
    let store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    let updated = store.update(crb).await?;
    Ok(Json(updated))
}

pub async fn delete_clusterrolebinding(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<ClusterRoleBinding>> {
    let store = ResourceStore::<ClusterRoleBinding>::new(state.storage.clone());
    let crb = store.get(None, &name).await?;

    if store.delete(None, &name).await? {
        match crb {
            Some(c) => Ok(Json(c)),
            None => Err(ApiError::NotFound {
                kind: "ClusterRoleBinding".to_string(),
                name,
            }),
        }
    } else {
        Err(ApiError::NotFound {
            kind: "ClusterRoleBinding".to_string(),
            name,
        })
    }
}

pub async fn patch_clusterrolebinding(
    state: State<AppState>,
    path: Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<ClusterRoleBinding>> {
    patch_cluster::<ClusterRoleBinding>(state, path, headers, body).await
}
