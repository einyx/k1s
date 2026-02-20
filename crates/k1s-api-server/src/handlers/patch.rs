//! PATCH request handlers for Kubernetes API compatibility
//!
//! Supports:
//! - JSON Patch (application/json-patch+json) - RFC 6902
//! - JSON Merge Patch (application/merge-patch+json) - RFC 7386
//! - Strategic Merge Patch (application/strategic-merge-patch+json) - K8s specific

use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::HeaderMap;
use axum::Json;
use json_patch::{patch as apply_json_patch, Patch};
use serde_json::Value;

use k1s_storage::ResourceStore;
use k1s_types::Resource;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Content types for different patch formats
const JSON_PATCH: &str = "application/json-patch+json";
const MERGE_PATCH: &str = "application/merge-patch+json";
const STRATEGIC_MERGE_PATCH: &str = "application/strategic-merge-patch+json";

/// Detect patch type from Content-Type header
fn detect_patch_type(headers: &HeaderMap) -> PatchType {
    headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            if ct.contains(JSON_PATCH) {
                PatchType::JsonPatch
            } else if ct.contains(STRATEGIC_MERGE_PATCH) {
                PatchType::StrategicMergePatch
            } else if ct.contains(MERGE_PATCH) {
                PatchType::MergePatch
            } else {
                // Default to merge patch for application/json
                PatchType::MergePatch
            }
        })
        .unwrap_or(PatchType::MergePatch)
}

#[derive(Debug, Clone, Copy)]
enum PatchType {
    JsonPatch,
    MergePatch,
    StrategicMergePatch,
}

/// Apply a JSON Patch (RFC 6902)
fn apply_rfc6902_patch(mut doc: Value, patch_value: Value) -> Result<Value, ApiError> {
    let patch: Patch = serde_json::from_value(patch_value)
        .map_err(|e| ApiError::BadRequest(format!("Invalid JSON Patch: {}", e)))?;

    apply_json_patch(&mut doc, &patch)
        .map_err(|e| ApiError::BadRequest(format!("Failed to apply JSON Patch: {}", e)))?;

    Ok(doc)
}

/// Apply a JSON Merge Patch (RFC 7386)
fn apply_merge_patch(doc: Value, patch: Value) -> Value {
    json_merge_patch(doc, patch)
}

/// Recursive JSON merge patch implementation per RFC 7386
fn json_merge_patch(target: Value, patch: Value) -> Value {
    match (target, patch) {
        (Value::Object(mut target_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                if patch_value.is_null() {
                    target_map.remove(&key);
                } else {
                    let target_value = target_map.remove(&key).unwrap_or(Value::Null);
                    target_map.insert(key, json_merge_patch(target_value, patch_value));
                }
            }
            Value::Object(target_map)
        }
        (_, patch) => patch,
    }
}

/// Apply Strategic Merge Patch (Kubernetes-specific)
/// For simplicity, we treat this similarly to merge patch but with special handling for arrays
fn apply_strategic_merge_patch(doc: Value, patch: Value) -> Value {
    strategic_merge(doc, patch)
}

/// Strategic merge implementation
/// Arrays are replaced by default, but can be merged using $patch directives
fn strategic_merge(target: Value, patch: Value) -> Value {
    match (target, patch) {
        (Value::Object(mut target_map), Value::Object(patch_map)) => {
            // Check for $patch directive
            if let Some(directive) = patch_map.get("$patch") {
                if directive == "delete" {
                    return Value::Null;
                }
                if directive == "replace" {
                    let mut result = patch_map.clone();
                    result.remove("$patch");
                    return Value::Object(result);
                }
            }

            for (key, patch_value) in patch_map {
                if key == "$patch" {
                    continue;
                }
                if patch_value.is_null() {
                    target_map.remove(&key);
                } else {
                    let target_value = target_map.remove(&key).unwrap_or(Value::Null);
                    let merged = strategic_merge(target_value, patch_value);
                    if !merged.is_null() {
                        target_map.insert(key, merged);
                    }
                }
            }
            Value::Object(target_map)
        }
        (Value::Array(mut target_arr), Value::Array(patch_arr)) => {
            // For arrays, check if items have $patch directives or merge keys
            // Default behavior: replace the array
            // If patch items have "name" field, try to merge by name

            let has_merge_key = patch_arr.iter().any(|v| {
                v.as_object().map(|o| o.contains_key("name")).unwrap_or(false)
            });

            if has_merge_key {
                // Merge arrays by "name" field
                for patch_item in patch_arr {
                    if let Some(patch_obj) = patch_item.as_object() {
                        if let Some(name) = patch_obj.get("name").and_then(|n| n.as_str()) {
                            // Check for delete directive
                            if patch_obj.get("$patch").map(|d| d == "delete").unwrap_or(false) {
                                target_arr.retain(|item| {
                                    item.get("name").and_then(|n| n.as_str()) != Some(name)
                                });
                                continue;
                            }

                            // Find and merge matching item
                            let mut found = false;
                            for target_item in target_arr.iter_mut() {
                                if target_item.get("name").and_then(|n| n.as_str()) == Some(name) {
                                    *target_item = strategic_merge(target_item.clone(), patch_item.clone());
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                target_arr.push(patch_item);
                            }
                        } else {
                            target_arr.push(patch_item);
                        }
                    }
                }
                Value::Array(target_arr)
            } else {
                // Replace array entirely
                Value::Array(patch_arr)
            }
        }
        (_, patch) => patch,
    }
}

/// Generic PATCH handler for namespaced resources
pub async fn patch_namespaced<R: Resource>(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<R>> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    // Get existing resource
    let existing = store.get(Some(&namespace), &name).await?.ok_or_else(|| ApiError::NotFound {
        kind: R::KIND.to_string(),
        name: name.clone(),
    })?;

    // Convert to JSON value
    let mut doc: Value = serde_json::to_value(&existing)
        .map_err(|e| ApiError::Internal(format!("Failed to serialize resource: {}", e)))?;

    // Parse patch body
    let patch_value: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("Invalid patch body: {}", e)))?;

    // Apply patch based on content type
    let patch_type = detect_patch_type(&headers);
    doc = match patch_type {
        PatchType::JsonPatch => apply_rfc6902_patch(doc, patch_value)?,
        PatchType::MergePatch => apply_merge_patch(doc, patch_value),
        PatchType::StrategicMergePatch => apply_strategic_merge_patch(doc, patch_value),
    };

    // Deserialize back to resource
    let mut patched: R = serde_json::from_value(doc)
        .map_err(|e| ApiError::BadRequest(format!("Patched resource is invalid: {}", e)))?;

    // Ensure namespace and name are preserved
    let meta = patched.metadata_mut();
    meta.namespace = Some(namespace);
    meta.name = name;

    // Update in storage
    let updated = store.update(patched).await?;

    Ok(Json(updated))
}

/// Generic PATCH handler for cluster-scoped resources
pub async fn patch_cluster<R: Resource>(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<Json<R>> {
    let store = ResourceStore::<R>::new(state.storage.clone());

    // Get existing resource
    let existing = store.get(None, &name).await?.ok_or_else(|| ApiError::NotFound {
        kind: R::KIND.to_string(),
        name: name.clone(),
    })?;

    // Convert to JSON value
    let mut doc: Value = serde_json::to_value(&existing)
        .map_err(|e| ApiError::Internal(format!("Failed to serialize resource: {}", e)))?;

    // Parse patch body
    let patch_value: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::BadRequest(format!("Invalid patch body: {}", e)))?;

    // Apply patch based on content type
    let patch_type = detect_patch_type(&headers);
    doc = match patch_type {
        PatchType::JsonPatch => apply_rfc6902_patch(doc, patch_value)?,
        PatchType::MergePatch => apply_merge_patch(doc, patch_value),
        PatchType::StrategicMergePatch => apply_strategic_merge_patch(doc, patch_value),
    };

    // Deserialize back to resource
    let mut patched: R = serde_json::from_value(doc)
        .map_err(|e| ApiError::BadRequest(format!("Patched resource is invalid: {}", e)))?;

    // Ensure name is preserved
    patched.metadata_mut().name = name;

    // Update in storage
    let updated = store.update(patched).await?;

    Ok(Json(updated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_patch() {
        let doc = json!({
            "metadata": {
                "name": "test",
                "labels": {"app": "old"}
            },
            "spec": {
                "replicas": 1
            }
        });

        let patch = json!({
            "metadata": {
                "labels": {"app": "new", "version": "v1"}
            },
            "spec": {
                "replicas": 3
            }
        });

        let result = apply_merge_patch(doc, patch);
        assert_eq!(result["metadata"]["labels"]["app"], "new");
        assert_eq!(result["metadata"]["labels"]["version"], "v1");
        assert_eq!(result["spec"]["replicas"], 3);
    }

    #[test]
    fn test_merge_patch_delete() {
        let doc = json!({
            "metadata": {
                "labels": {"app": "test", "remove": "me"}
            }
        });

        let patch = json!({
            "metadata": {
                "labels": {"remove": null}
            }
        });

        let result = apply_merge_patch(doc, patch);
        assert!(result["metadata"]["labels"].get("remove").is_none());
        assert_eq!(result["metadata"]["labels"]["app"], "test");
    }

    #[test]
    fn test_strategic_merge_arrays_by_name() {
        let doc = json!({
            "containers": [
                {"name": "app", "image": "nginx:1.0"},
                {"name": "sidecar", "image": "proxy:1.0"}
            ]
        });

        let patch = json!({
            "containers": [
                {"name": "app", "image": "nginx:2.0"}
            ]
        });

        let result = apply_strategic_merge_patch(doc, patch);
        let containers = result["containers"].as_array().unwrap();
        assert_eq!(containers.len(), 2);
        assert_eq!(containers[0]["image"], "nginx:2.0");
        assert_eq!(containers[1]["image"], "proxy:1.0");
    }
}
