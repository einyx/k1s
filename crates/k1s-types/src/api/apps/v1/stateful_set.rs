//! StatefulSet resource type

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};
use crate::PersistentVolumeClaimSpec;

use super::deployment::{LabelSelector, PodTemplateSpec};

/// StatefulSet represents a set of pods with consistent identities
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSet {
    #[serde(default = "StatefulSet::api_version")]
    pub api_version: String,
    #[serde(default = "StatefulSet::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<StatefulSetSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<StatefulSetStatus>,
}

impl StatefulSet {
    fn api_version() -> String {
        "apps/v1".to_string()
    }

    fn kind() -> String {
        "StatefulSet".to_string()
    }
}

impl Resource for StatefulSet {
    const API_VERSION: &'static str = "apps/v1";
    const KIND: &'static str = "StatefulSet";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "statefulsets";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// StatefulSetSpec is the specification of a StatefulSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetSpec {
    /// Replicas is the desired number of replicas
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i32>,
    /// Selector is a label query over pods that should match the replica count
    pub selector: LabelSelector,
    /// Template is the pod template
    pub template: PodTemplateSpec,
    /// VolumeClaimTemplates is a list of claims that pods are allowed to reference
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume_claim_templates: Vec<PersistentVolumeClaimTemplate>,
    /// ServiceName is the name of the service that governs this StatefulSet
    pub service_name: String,
    /// PodManagementPolicy controls how pods are created during initial scale up
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_management_policy: Option<PodManagementPolicyType>,
    /// UpdateStrategy indicates the StatefulSetUpdateStrategy that will be employed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_strategy: Option<StatefulSetUpdateStrategy>,
    /// RevisionHistoryLimit is the maximum number of revisions that will be maintained
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_history_limit: Option<i32>,
    /// MinReadySeconds minimum seconds a pod should be ready before considered available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_ready_seconds: Option<i32>,
    /// PersistentVolumeClaimRetentionPolicy describes the policy for PVC retention
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_volume_claim_retention_policy: Option<StatefulSetPersistentVolumeClaimRetentionPolicy>,
    /// Ordinals controls the ordinal index of pods in the StatefulSet
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinals: Option<StatefulSetOrdinals>,
}

/// StatefulSetStatus represents the current state of a StatefulSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetStatus {
    /// ObservedGeneration is the most recent generation observed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    /// Replicas is the number of pods created by the StatefulSet controller
    #[serde(default)]
    pub replicas: i32,
    /// ReadyReplicas is the number of pods with a Ready condition
    #[serde(default)]
    pub ready_replicas: i32,
    /// CurrentReplicas is the number of pods created by the controller with the current version
    #[serde(default)]
    pub current_replicas: i32,
    /// UpdatedReplicas is the number of pods created by the controller with the updated version
    #[serde(default)]
    pub updated_replicas: i32,
    /// CurrentRevision is the version label of the current pods
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<String>,
    /// UpdateRevision is the version label of the updated pods
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_revision: Option<String>,
    /// CollisionCount is the count of hash collisions
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collision_count: Option<i32>,
    /// Conditions represent the latest observations of a StatefulSet's state
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<StatefulSetCondition>,
    /// AvailableReplicas is the number of pods with Ready conditions for minReadySeconds
    #[serde(default)]
    pub available_replicas: i32,
}

/// StatefulSetCondition describes the state of a StatefulSet at a certain point
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// PersistentVolumeClaimTemplate is used to produce PVCs
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimTemplate {
    #[serde(default)]
    pub metadata: ObjectMeta,
    pub spec: PersistentVolumeClaimSpec,
}

/// PodManagementPolicyType defines the policy for creating pods
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PodManagementPolicyType {
    OrderedReady,
    Parallel,
}

impl Default for PodManagementPolicyType {
    fn default() -> Self {
        PodManagementPolicyType::OrderedReady
    }
}

/// StatefulSetUpdateStrategy indicates the strategy for updating a StatefulSet
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetUpdateStrategy {
    /// Type indicates the type of the StatefulSetUpdateStrategy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<StatefulSetUpdateStrategyType>,
    /// RollingUpdate is used to communicate parameters when Type is RollingUpdate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rolling_update: Option<RollingUpdateStatefulSetStrategy>,
}

/// StatefulSetUpdateStrategyType is a strategy for StatefulSet updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StatefulSetUpdateStrategyType {
    RollingUpdate,
    OnDelete,
}

impl Default for StatefulSetUpdateStrategyType {
    fn default() -> Self {
        StatefulSetUpdateStrategyType::RollingUpdate
    }
}

/// RollingUpdateStatefulSetStrategy is used for rolling update
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateStatefulSetStrategy {
    /// Partition indicates the ordinal at which the StatefulSet should be partitioned
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition: Option<i32>,
    /// MaxUnavailable is the maximum number of pods that can be unavailable during update
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_unavailable: Option<i32>,
}

/// StatefulSetPersistentVolumeClaimRetentionPolicy describes the policy for PVC retention
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetPersistentVolumeClaimRetentionPolicy {
    /// WhenDeleted specifies what happens to PVCs when the StatefulSet is deleted
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_deleted: Option<PersistentVolumeClaimRetentionPolicyType>,
    /// WhenScaled specifies what happens to PVCs when the StatefulSet is scaled down
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_scaled: Option<PersistentVolumeClaimRetentionPolicyType>,
}

/// PersistentVolumeClaimRetentionPolicyType is a policy for PVC retention
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumeClaimRetentionPolicyType {
    Retain,
    Delete,
}

impl Default for PersistentVolumeClaimRetentionPolicyType {
    fn default() -> Self {
        PersistentVolumeClaimRetentionPolicyType::Retain
    }
}

/// StatefulSetOrdinals describes the policy for assigning ordinals to pods
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatefulSetOrdinals {
    /// Start is the number representing the first replica's index
    #[serde(default)]
    pub start: i32,
}
