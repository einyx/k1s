//! DaemonSet resource type (apps/v1)
//! Used for Swarm "global" mode services that run on every node.

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};

use super::deployment::{LabelSelector, PodTemplateSpec};

/// DaemonSetSpec describes a DaemonSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSetSpec {
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
    /// Minimum ready seconds before the pod is considered available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ready_seconds: Option<i32>,
    /// Revision history limit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_history_limit: Option<i32>,
    /// Update strategy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_strategy: Option<DaemonSetUpdateStrategy>,
}

/// DaemonSetUpdateStrategy describes how to update a DaemonSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSetUpdateStrategy {
    /// Type of update strategy (RollingUpdate or OnDelete)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// Rolling update config
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolling_update: Option<RollingUpdateDaemonSet>,
}

/// Rolling update config for DaemonSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateDaemonSet {
    /// Max unavailable pods during update
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_unavailable: Option<i32>,
    /// Max surge pods during update
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_surge: Option<i32>,
}

/// DaemonSetStatus represents the current state of a DaemonSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSetStatus {
    /// Current number of nodes with daemon pods scheduled
    #[serde(default)]
    pub current_number_scheduled: i32,
    /// Number of nodes with desired pods
    #[serde(default)]
    pub desired_number_scheduled: i32,
    /// Number of nodes with ready daemon pods
    #[serde(default)]
    pub number_ready: i32,
    /// Number of nodes with available daemon pods
    #[serde(default)]
    pub number_available: i32,
    /// Number of nodes with unavailable daemon pods
    #[serde(default)]
    pub number_unavailable: i32,
    /// Number of nodes with misscheduled daemon pods
    #[serde(default)]
    pub number_misscheduled: i32,
    /// Updated number scheduled
    #[serde(default)]
    pub updated_number_scheduled: i32,
    /// Observed generation
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    /// Conditions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<DaemonSetCondition>,
    /// Collision count
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_count: Option<i32>,
}

/// DaemonSetCondition describes the state of a DaemonSet at a certain point
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSetCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// DaemonSet ensures one Pod runs on every node (Swarm global equivalent)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSet {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<DaemonSetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<DaemonSetStatus>,
}

impl Resource for DaemonSet {
    const API_VERSION: &'static str = "apps/v1";
    const KIND: &'static str = "DaemonSet";
    const PLURAL: &'static str = "daemonsets";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}
