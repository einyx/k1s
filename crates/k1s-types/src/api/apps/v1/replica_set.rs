//! ReplicaSet resource type (apps/v1)

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::deployment::{LabelSelector, PodTemplateSpec};

/// ReplicaSetSpec describes the desired state of a ReplicaSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetSpec {
    /// Desired number of pod replicas
    #[serde(default)]
    pub replicas: Option<i32>,
    /// Label selector for pods
    pub selector: LabelSelector,
    /// Pod template
    pub template: PodTemplateSpec,
    /// Minimum seconds a pod must be ready without crashing to be considered available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ready_seconds: Option<i32>,
}

/// ReplicaSetStatus reports observed state
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetStatus {
    /// Total number of pods targeted by this ReplicaSet
    #[serde(default)]
    pub replicas: i32,
    /// Number of pods that have reached Ready condition
    #[serde(default)]
    pub ready_replicas: i32,
    /// Number of available replicas
    #[serde(default)]
    pub available_replicas: i32,
    /// The generation observed by the controller
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    /// Represents the latest available observations
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ReplicaSetCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSetCondition {
    pub r#type: ReplicaSetConditionType,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReplicaSetConditionType {
    ReplicaFailure,
}

/// ReplicaSet ensures a specified number of pod replicas are running
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplicaSet {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<ReplicaSetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ReplicaSetStatus>,
}

impl Resource for ReplicaSet {
    const API_VERSION: &'static str = "apps/v1";
    const KIND: &'static str = "ReplicaSet";
    const PLURAL: &'static str = "replicasets";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl ReplicaSet {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("apps/v1", "ReplicaSet"),
            metadata: ObjectMeta::new(name),
            spec: None,
            status: None,
        }
    }

    pub fn namespaced(name: &str, namespace: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("apps/v1", "ReplicaSet"),
            metadata: ObjectMeta::namespaced(name, namespace),
            spec: None,
            status: None,
        }
    }

    /// Get desired replica count (defaults to 1)
    pub fn desired_replicas(&self) -> i32 {
        self.spec.as_ref().and_then(|s| s.replicas).unwrap_or(1)
    }

    /// Check if labels match the selector
    pub fn matches_selector(&self, labels: &BTreeMap<String, String>) -> bool {
        if let Some(spec) = &self.spec {
            for (key, value) in &spec.selector.match_labels {
                if labels.get(key) != Some(value) {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}
