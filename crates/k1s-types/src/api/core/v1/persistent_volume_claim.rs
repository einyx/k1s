//! PersistentVolumeClaim resource type

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::api::apps::v1::LabelSelector;
use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::persistent_volume::{PersistentVolumeAccessMode, PersistentVolumeMode};

/// PersistentVolumeClaim (PVC) is a user's request for a persistent volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaim {
    #[serde(default = "PersistentVolumeClaim::api_version")]
    pub api_version: String,
    #[serde(default = "PersistentVolumeClaim::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<PersistentVolumeClaimSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PersistentVolumeClaimStatus>,
}

impl PersistentVolumeClaim {
    fn api_version() -> String {
        "v1".to_string()
    }

    fn kind() -> String {
        "PersistentVolumeClaim".to_string()
    }
}

impl Resource for PersistentVolumeClaim {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "PersistentVolumeClaim";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "persistentvolumeclaims";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// PersistentVolumeClaimSpec describes the storage requested by the user
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimSpec {
    /// AccessModes contains the desired access modes the volume should have
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_modes: Vec<PersistentVolumeAccessMode>,
    /// Selector is a label query over volumes to consider for binding
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<LabelSelector>,
    /// Resources represents the minimum resources the volume should have
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceRequirements>,
    /// VolumeName is the binding reference to the PersistentVolume
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    /// StorageClassName is the name of the StorageClass required by the claim
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class_name: Option<String>,
    /// VolumeMode defines what type of volume is required by the claim
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_mode: Option<PersistentVolumeMode>,
    /// DataSource field can be used to specify a PVC or VolumeSnapshot to pre-populate the PVC
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<TypedLocalObjectReference>,
    /// DataSourceRef is similar to DataSource but allows any object
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source_ref: Option<TypedObjectReference>,
}

/// PersistentVolumeClaimStatus is the current status of a PVC
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimStatus {
    /// Phase represents the current phase of PersistentVolumeClaim
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PersistentVolumeClaimPhase>,
    /// AccessModes contains the actual access modes the volume has
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_modes: Vec<PersistentVolumeAccessMode>,
    /// Capacity represents the actual capacity of the underlying volume
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capacity: BTreeMap<String, String>,
    /// Conditions is the current condition of the PVC
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<PersistentVolumeClaimCondition>,
}

/// PersistentVolumeClaimPhase represents the current phase of a PVC
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumeClaimPhase {
    Pending,
    Bound,
    Lost,
}

/// PersistentVolumeClaimCondition contains details about the state of the PVC
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_probe_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
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

/// TypedLocalObjectReference contains enough information to locate a typed referenced object inside the same namespace
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypedLocalObjectReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_group: Option<String>,
    pub kind: String,
    pub name: String,
}

/// TypedObjectReference contains enough information to locate a typed referenced object
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TypedObjectReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_group: Option<String>,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}
