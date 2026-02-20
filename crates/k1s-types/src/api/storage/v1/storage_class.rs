//! StorageClass resource type

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};
use crate::PersistentVolumeReclaimPolicy;

/// StorageClass describes the parameters for a class of storage
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StorageClass {
    #[serde(default = "StorageClass::api_version")]
    pub api_version: String,
    #[serde(default = "StorageClass::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Provisioner indicates the type of the provisioner
    pub provisioner: String,
    /// Parameters holds the parameters for the provisioner
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    /// ReclaimPolicy dynamically provisioned PersistentVolumes of this storage class are created with
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reclaim_policy: Option<PersistentVolumeReclaimPolicy>,
    /// MountOptions controls the mountOptions for dynamically provisioned PersistentVolumes
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_options: Vec<String>,
    /// AllowVolumeExpansion shows whether the storage class allow volume expand
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_volume_expansion: Option<bool>,
    /// VolumeBindingMode indicates how PersistentVolumeClaims should be provisioned and bound
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_binding_mode: Option<VolumeBindingMode>,
    /// AllowedTopologies restrict the node topologies where volumes can be dynamically provisioned
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_topologies: Vec<TopologySelectorTerm>,
}

impl StorageClass {
    fn api_version() -> String {
        "storage.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "StorageClass".to_string()
    }
}

impl Resource for StorageClass {
    const API_VERSION: &'static str = "storage.k8s.io/v1";
    const KIND: &'static str = "StorageClass";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "storageclasses";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// VolumeBindingMode indicates how PersistentVolumeClaims should be bound
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VolumeBindingMode {
    Immediate,
    WaitForFirstConsumer,
}

impl Default for VolumeBindingMode {
    fn default() -> Self {
        VolumeBindingMode::Immediate
    }
}

/// TopologySelectorTerm defines a term for topology selection
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopologySelectorTerm {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_label_expressions: Vec<TopologySelectorLabelRequirement>,
}

/// TopologySelectorLabelRequirement is a label requirement for topology selection
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TopologySelectorLabelRequirement {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}
