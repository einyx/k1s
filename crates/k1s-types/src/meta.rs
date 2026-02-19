//! Kubernetes metadata types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// TypeMeta describes the API version and kind of a resource
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypeMeta {
    /// API version (e.g., "v1", "apps/v1")
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_version: String,

    /// Kind of resource (e.g., "Pod", "Deployment")
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
}

impl TypeMeta {
    pub fn new(api_version: &str, kind: &str) -> Self {
        Self {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
        }
    }
}

/// ObjectMeta is metadata attached to all persisted resources
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    /// Name must be unique within a namespace
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,

    /// Namespace defines the space within which name must be unique
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// UID is the unique identifier for this resource
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub uid: String,

    /// ResourceVersion is used for optimistic concurrency control
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resource_version: String,

    /// Generation is incremented by the system on spec changes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<i64>,

    /// CreationTimestamp is the time at which the resource was created
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creation_timestamp: Option<DateTime<Utc>>,

    /// DeletionTimestamp is the time after which the resource will be deleted
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_timestamp: Option<DateTime<Utc>>,

    /// DeletionGracePeriodSeconds allows graceful termination
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_grace_period_seconds: Option<i64>,

    /// Labels are key/value pairs for organizing resources
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,

    /// Annotations are arbitrary key/value pairs
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,

    /// OwnerReferences lists objects that own this resource
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_references: Vec<OwnerReference>,

    /// Finalizers must be empty before the resource is deleted
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finalizers: Vec<String>,
}

impl ObjectMeta {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    pub fn namespaced(name: &str, namespace: &str) -> Self {
        Self {
            name: name.to_string(),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        }
    }

    /// Get effective namespace (defaults to "default")
    pub fn effective_namespace(&self) -> &str {
        self.namespace.as_deref().unwrap_or("default")
    }
}

/// OwnerReference contains information about an owning object
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OwnerReference {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub uid: String,
    #[serde(default)]
    pub controller: Option<bool>,
    #[serde(default)]
    pub block_owner_deletion: Option<bool>,
}

/// ListMeta describes metadata for list responses
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ListMeta {
    /// ResourceVersion at the time the list was generated
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resource_version: String,

    /// Continue token for pagination
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continue_: Option<String>,

    /// Total remaining items (when using pagination)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_item_count: Option<i64>,
}
