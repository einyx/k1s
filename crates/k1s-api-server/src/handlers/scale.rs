//! Scale subresource handlers

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use k1s_storage::ResourceStore;
use k1s_types::{apps_v1::StatefulSet, Deployment, ReplicaSet};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Scale subresource for controlling replica counts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scale {
    #[serde(default)]
    pub api_version: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub metadata: ScaleMetadata,
    #[serde(default)]
    pub spec: ScaleSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ScaleStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resource_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleSpec {
    #[serde(default)]
    pub replicas: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleStatus {
    pub replicas: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub selector: String,
}

impl Scale {
    pub fn new(name: &str, namespace: Option<&str>, replicas: i32, current_replicas: i32) -> Self {
        Self {
            api_version: "autoscaling/v1".to_string(),
            kind: "Scale".to_string(),
            metadata: ScaleMetadata {
                name: name.to_string(),
                namespace: namespace.map(String::from),
                ..Default::default()
            },
            spec: ScaleSpec { replicas },
            status: Some(ScaleStatus {
                replicas: current_replicas,
                selector: String::new(),
            }),
        }
    }
}

// Deployment scale handlers

pub async fn get_deployment_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(deployment) => {
            let replicas = deployment.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = deployment.status.as_ref().map(|s| s.replicas).unwrap_or(0);
            let mut scale = Scale::new(&name, Some(&namespace), replicas, current);
            scale.metadata.resource_version = deployment.metadata.resource_version.clone();
            scale.metadata.uid = deployment.metadata.uid.clone();
            Ok(Json(scale))
        }
        None => Err(ApiError::NotFound {
            kind: "Deployment".to_string(),
            name,
        }),
    }
}

pub async fn update_deployment_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Scale>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<Deployment>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(mut deployment) => {
            // Update replicas
            if let Some(spec) = &mut deployment.spec {
                spec.replicas = Some(scale.spec.replicas);
            }

            let updated = store.update(deployment).await?;
            let replicas = updated.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = updated.status.as_ref().map(|s| s.replicas).unwrap_or(0);

            let mut result = Scale::new(&name, Some(&namespace), replicas, current);
            result.metadata.resource_version = updated.metadata.resource_version.clone();
            result.metadata.uid = updated.metadata.uid.clone();
            Ok(Json(result))
        }
        None => Err(ApiError::NotFound {
            kind: "Deployment".to_string(),
            name,
        }),
    }
}

// ReplicaSet scale handlers

pub async fn get_replicaset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(rs) => {
            let replicas = rs.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = rs.status.as_ref().map(|s| s.replicas).unwrap_or(0);
            let mut scale = Scale::new(&name, Some(&namespace), replicas, current);
            scale.metadata.resource_version = rs.metadata.resource_version.clone();
            scale.metadata.uid = rs.metadata.uid.clone();
            Ok(Json(scale))
        }
        None => Err(ApiError::NotFound {
            kind: "ReplicaSet".to_string(),
            name,
        }),
    }
}

pub async fn update_replicaset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Scale>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<ReplicaSet>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(mut rs) => {
            // Update replicas
            if let Some(spec) = &mut rs.spec {
                spec.replicas = Some(scale.spec.replicas);
            }

            let updated = store.update(rs).await?;
            let replicas = updated.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = updated.status.as_ref().map(|s| s.replicas).unwrap_or(0);

            let mut result = Scale::new(&name, Some(&namespace), replicas, current);
            result.metadata.resource_version = updated.metadata.resource_version.clone();
            result.metadata.uid = updated.metadata.uid.clone();
            Ok(Json(result))
        }
        None => Err(ApiError::NotFound {
            kind: "ReplicaSet".to_string(),
            name,
        }),
    }
}

// StatefulSet scale handlers

pub async fn get_statefulset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(sts) => {
            let replicas = sts.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = sts.status.as_ref().map(|s| s.replicas).unwrap_or(0);
            let mut scale = Scale::new(&name, Some(&namespace), replicas, current);
            scale.metadata.resource_version = sts.metadata.resource_version.clone();
            scale.metadata.uid = sts.metadata.uid.clone();
            Ok(Json(scale))
        }
        None => Err(ApiError::NotFound {
            kind: "StatefulSet".to_string(),
            name,
        }),
    }
}

pub async fn update_statefulset_scale(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Json(scale): Json<Scale>,
) -> ApiResult<Json<Scale>> {
    let store = ResourceStore::<StatefulSet>::new(state.storage.clone());

    match store.get(Some(&namespace), &name).await? {
        Some(mut sts) => {
            // Update replicas
            if let Some(spec) = &mut sts.spec {
                spec.replicas = Some(scale.spec.replicas);
            }

            let updated = store.update(sts).await?;
            let replicas = updated.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1);
            let current = updated.status.as_ref().map(|s| s.replicas).unwrap_or(0);

            let mut result = Scale::new(&name, Some(&namespace), replicas, current);
            result.metadata.resource_version = updated.metadata.resource_version.clone();
            result.metadata.uid = updated.metadata.uid.clone();
            Ok(Json(result))
        }
        None => Err(ApiError::NotFound {
            kind: "StatefulSet".to_string(),
            name,
        }),
    }
}
