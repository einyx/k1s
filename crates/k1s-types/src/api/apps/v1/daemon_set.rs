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
