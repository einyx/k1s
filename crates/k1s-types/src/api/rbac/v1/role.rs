//! Role resource type

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

/// Role contains rules that represent a set of permissions within a namespace
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    #[serde(default = "Role::api_version")]
    pub api_version: String,
    #[serde(default = "Role::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Rules holds all the PolicyRules for this Role
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<PolicyRule>,
}

impl Role {
    fn api_version() -> String {
        "rbac.authorization.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "Role".to_string()
    }
}

impl Resource for Role {
    const API_VERSION: &'static str = "rbac.authorization.k8s.io/v1";
    const KIND: &'static str = "Role";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "roles";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// PolicyRule holds information that describes a policy rule
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    /// Verbs is a list of allowed actions (get, list, watch, create, update, patch, delete, deletecollection)
    #[serde(default)]
    pub verbs: Vec<String>,
    /// APIGroups is the name of the APIGroup that contains the resources ("" for core, "apps", "batch", etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_groups: Vec<String>,
    /// Resources is a list of resources this rule applies to (pods, deployments, etc.)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,
    /// ResourceNames is an optional list of names that the rule applies to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_names: Vec<String>,
    /// NonResourceURLs is a set of partial urls a user should have access to (e.g., /healthz)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub non_resource_urls: Vec<String>,
}

impl PolicyRule {
    /// Check if this rule matches the given request
    pub fn matches(&self, api_group: &str, resource: &str, verb: &str, name: Option<&str>) -> bool {
        // Check verb
        if !self.verbs.iter().any(|v| v == "*" || v == verb) {
            return false;
        }

        // Check API group
        if !self.api_groups.is_empty()
            && !self.api_groups.iter().any(|g| g == "*" || g == api_group)
        {
            return false;
        }

        // Check resource
        if !self.resources.is_empty()
            && !self.resources.iter().any(|r| r == "*" || r == resource)
        {
            return false;
        }

        // Check resource name if specified
        if !self.resource_names.is_empty() {
            if let Some(n) = name {
                if !self.resource_names.iter().any(|rn| rn == n) {
                    return false;
                }
            }
        }

        true
    }
}
