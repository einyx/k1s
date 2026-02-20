//! ClusterRoleBinding resource type

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::{RoleRef, Subject};

/// ClusterRoleBinding references a ClusterRole and binds subjects to it cluster-wide
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRoleBinding {
    #[serde(default = "ClusterRoleBinding::api_version")]
    pub api_version: String,
    #[serde(default = "ClusterRoleBinding::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Subjects holds references to the objects the role applies to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subjects: Vec<Subject>,
    /// RoleRef references the ClusterRole for this binding
    pub role_ref: RoleRef,
}

impl ClusterRoleBinding {
    fn api_version() -> String {
        "rbac.authorization.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "ClusterRoleBinding".to_string()
    }
}

impl Resource for ClusterRoleBinding {
    const API_VERSION: &'static str = "rbac.authorization.k8s.io/v1";
    const KIND: &'static str = "ClusterRoleBinding";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "clusterrolebindings";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}
