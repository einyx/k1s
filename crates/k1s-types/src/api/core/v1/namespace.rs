//! Namespace resource types

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};

/// Namespace provides a scope for names
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<NamespaceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<NamespaceStatus>,
}

impl Resource for Namespace {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Namespace";
    const PLURAL: &'static str = "namespaces";
    const SCOPE: ResourceScope = ResourceScope::Cluster;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl Namespace {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Namespace"),
            metadata: ObjectMeta::new(name),
            spec: Some(NamespaceSpec::default()),
            status: None,
        }
    }
}

/// NamespaceSpec is the specification for a namespace
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceSpec {
    /// Finalizers is an opaque list of values that must be empty to permanently remove object
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub finalizers: Vec<String>,
}

/// NamespaceStatus is the status for a namespace
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceStatus {
    /// Phase of the namespace
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<NamespacePhase>,

    /// Conditions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<NamespaceCondition>,
}

/// Phase of a namespace
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum NamespacePhase {
    #[default]
    Active,
    Terminating,
}

/// Namespace condition
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
