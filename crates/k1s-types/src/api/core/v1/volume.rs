//! Volume types

use serde::{Deserialize, Serialize};

/// Volume represents a named volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    #[serde(flatten)]
    pub source: VolumeSource,
}

/// VolumeSource represents the source of a volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_dir: Option<EmptyDirVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_path: Option<HostPathVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map: Option<ConfigMapVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_volume_claim: Option<PersistentVolumeClaimVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downward_api: Option<DownwardAPIVolumeSource>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected: Option<ProjectedVolumeSource>,
}

/// EmptyDir volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EmptyDirVolumeSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<StorageMedium>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_limit: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum StorageMedium {
    #[default]
    #[serde(rename = "")]
    Default,
    Memory,
    HugePages,
}

/// HostPath volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostPathVolumeSource {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<HostPathType>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub enum HostPathType {
    #[default]
    #[serde(rename = "")]
    Unset,
    DirectoryOrCreate,
    Directory,
    FileOrCreate,
    File,
    Socket,
    CharDevice,
    BlockDevice,
}

/// ConfigMap volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapVolumeSource {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<KeyToPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<i32>,
    #[serde(default)]
    pub optional: Option<bool>,
}

/// Secret volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretVolumeSource {
    pub secret_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<KeyToPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<i32>,
    #[serde(default)]
    pub optional: Option<bool>,
}

/// Key to path mapping
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyToPath {
    pub key: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<i32>,
}

/// PVC volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistentVolumeClaimVolumeSource {
    pub claim_name: String,
    #[serde(default)]
    pub read_only: bool,
}

/// Downward API volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownwardAPIVolumeSource {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<DownwardAPIVolumeFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownwardAPIVolumeFile {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_ref: Option<super::container::ObjectFieldSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_field_ref: Option<super::container::ResourceFieldSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<i32>,
}

/// Projected volume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectedVolumeSource {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<VolumeProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VolumeProjection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_map: Option<ConfigMapProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downward_api: Option<DownwardAPIProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_account_token: Option<ServiceAccountTokenProjection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SecretProjection {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<KeyToPath>,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMapProjection {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<KeyToPath>,
    #[serde(default)]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownwardAPIProjection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<DownwardAPIVolumeFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountTokenProjection {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_seconds: Option<i64>,
}
