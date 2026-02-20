//! Common types shared across core v1 resources

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::resource::Resource;

/// ObjectReference contains enough information to let you locate the referenced object
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
}

impl ObjectReference {
    /// Create a reference from resource metadata
    pub fn from_resource<R: Resource>(resource: &R, api_version: &str, kind: &str) -> Self {
        let meta = resource.metadata();
        Self {
            api_version: Some(api_version.to_string()),
            kind: Some(kind.to_string()),
            name: Some(meta.name.clone()),
            namespace: meta.namespace.clone(),
            uid: Some(meta.uid.clone()),
            resource_version: Some(meta.resource_version.clone()),
            field_path: None,
        }
    }

    /// Create an object reference from a Pod
    pub fn from_pod(
        name: &str,
        namespace: &str,
        uid: &str,
        resource_version: &str,
    ) -> Self {
        Self {
            api_version: Some("v1".to_string()),
            kind: Some("Pod".to_string()),
            name: Some(name.to_string()),
            namespace: Some(namespace.to_string()),
            uid: Some(uid.to_string()),
            resource_version: Some(resource_version.to_string()),
            field_path: None,
        }
    }
}

/// LocalObjectReference contains enough information to let you locate a local object
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalObjectReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// NodeSelector represents a node selector
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelector {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_selector_terms: Vec<NodeSelectorTerm>,
}

/// NodeSelectorTerm defines node selector requirements
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelectorTerm {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_expressions: Vec<NodeSelectorRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_fields: Vec<NodeSelectorRequirement>,
}

/// NodeSelectorRequirement is a requirement for a node selector
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// ResourceRequirements describes compute resource requirements
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRequirements {
    /// Limits describes the maximum amount of resources allowed
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub limits: BTreeMap<String, String>,
    /// Requests describes the minimum amount of resources required
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub requests: BTreeMap<String, String>,
}
