//! PersistentVolume resource type

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::volume::HostPathVolumeSource;

/// PersistentVolume (PV) is a storage resource provisioned by an administrator
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolume {
    #[serde(default = "PersistentVolume::api_version")]
    pub api_version: String,
    #[serde(default = "PersistentVolume::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<PersistentVolumeSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<PersistentVolumeStatus>,
}

impl PersistentVolume {
    fn api_version() -> String {
        "v1".to_string()
    }

    fn kind() -> String {
        "PersistentVolume".to_string()
    }
}

impl Resource for PersistentVolume {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "PersistentVolume";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "persistentvolumes";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// PersistentVolumeSpec is the specification of a persistent volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeSpec {
    /// Capacity is the description of the persistent volume's resources
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub capacity: BTreeMap<String, String>,
    /// AccessModes contains the access modes the volume can be mounted with
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_modes: Vec<PersistentVolumeAccessMode>,
    /// ClaimRef is part of a bi-directional binding between PV and PVC
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_ref: Option<ObjectReference>,
    /// PersistentVolumeReclaimPolicy defines what happens to a PV when released
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_volume_reclaim_policy: Option<PersistentVolumeReclaimPolicy>,
    /// StorageClassName is the name of the StorageClass this PV belongs to
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_class_name: Option<String>,
    /// MountOptions is the list of mount options
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mount_options: Vec<String>,
    /// VolumeMode defines if a volume is intended to be used with a filesystem or block device
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_mode: Option<PersistentVolumeMode>,
    /// NodeAffinity defines constraints that limit what nodes this volume can be accessed from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_affinity: Option<VolumeNodeAffinity>,
    // Volume source (one of these should be set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_path: Option<HostPathVolumeSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<LocalVolumeSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nfs: Option<NFSVolumeSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csi: Option<CSIPersistentVolumeSource>,
}

/// PersistentVolumeStatus is the current status of a persistent volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeStatus {
    /// Phase indicates if a volume is available, bound to a claim, or released
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PersistentVolumePhase>,
    /// Message is a human-readable message indicating details about why the volume is in this state
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Reason is a brief string that describes any failure
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// PersistentVolumeAccessMode defines the access mode for a PV
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumeAccessMode {
    ReadWriteOnce,
    ReadOnlyMany,
    ReadWriteMany,
    ReadWriteOncePod,
}

/// PersistentVolumeReclaimPolicy describes a policy for PV reclaim
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumeReclaimPolicy {
    Retain,
    Recycle,
    Delete,
}

impl Default for PersistentVolumeReclaimPolicy {
    fn default() -> Self {
        PersistentVolumeReclaimPolicy::Retain
    }
}

/// PersistentVolumeMode describes how a volume is intended to be consumed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumeMode {
    Filesystem,
    Block,
}

impl Default for PersistentVolumeMode {
    fn default() -> Self {
        PersistentVolumeMode::Filesystem
    }
}

/// PersistentVolumePhase indicates the phase of a PV
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PersistentVolumePhase {
    Available,
    Bound,
    Released,
    Failed,
    Pending,
}

/// ObjectReference for ClaimRef
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

/// VolumeNodeAffinity defines constraints on which nodes a volume can be accessed from
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeNodeAffinity {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<NodeSelector>,
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

// Volume sources

/// LocalVolumeSource represents a local storage resource
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalVolumeSource {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,
}

/// NFSVolumeSource represents an NFS mount
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NFSVolumeSource {
    pub server: String,
    pub path: String,
    #[serde(default)]
    pub read_only: bool,
}

/// CSIPersistentVolumeSource represents a CSI volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CSIPersistentVolumeSource {
    pub driver: String,
    pub volume_handle: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub volume_attributes: BTreeMap<String, String>,
}
