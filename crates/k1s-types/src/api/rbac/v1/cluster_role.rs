//! ClusterRole resource type

use serde::{Deserialize, Serialize};

use crate::api::apps::v1::LabelSelector;
use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::PolicyRule;

/// ClusterRole is a cluster-level role that contains rules
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClusterRole {
    #[serde(default = "ClusterRole::api_version")]
    pub api_version: String,
    #[serde(default = "ClusterRole::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Rules holds all the PolicyRules for this ClusterRole
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PolicyRule>,
    /// AggregationRule describes how to build the Rules for this ClusterRole
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation_rule: Option<AggregationRule>,
}

impl ClusterRole {
    fn api_version() -> String {
        "rbac.authorization.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "ClusterRole".to_string()
    }
}

impl Resource for ClusterRole {
    const API_VERSION: &'static str = "rbac.authorization.k8s.io/v1";
    const KIND: &'static str = "ClusterRole";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "clusterroles";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// AggregationRule describes how to locate ClusterRoles to aggregate into the ClusterRole
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AggregationRule {
    /// ClusterRoleSelectors holds a list of selectors which will be used to find ClusterRoles and create the rules
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cluster_role_selectors: Vec<LabelSelector>,
}

