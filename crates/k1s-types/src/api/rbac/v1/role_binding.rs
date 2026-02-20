//! RoleBinding resource type

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

/// RoleBinding references a role and binds subjects to it
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleBinding {
    #[serde(default = "RoleBinding::api_version")]
    pub api_version: String,
    #[serde(default = "RoleBinding::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Subjects holds references to the objects the role applies to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subjects: Vec<Subject>,
    /// RoleRef references the role for this binding
    pub role_ref: RoleRef,
}

impl RoleBinding {
    fn api_version() -> String {
        "rbac.authorization.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "RoleBinding".to_string()
    }
}

impl Resource for RoleBinding {
    const API_VERSION: &'static str = "rbac.authorization.k8s.io/v1";
    const KIND: &'static str = "RoleBinding";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "rolebindings";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// Subject contains a reference to the object or user identities a role binding applies to
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    /// Kind of object being referenced (User, Group, ServiceAccount)
    pub kind: String,
    /// API group of the referenced subject (for ServiceAccount, this is "")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_group: Option<String>,
    /// Name of the object being referenced
    pub name: String,
    /// Namespace of the referenced object (required for ServiceAccount)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

impl Subject {
    /// Create a new ServiceAccount subject
    pub fn service_account(name: &str, namespace: &str) -> Self {
        Self {
            kind: "ServiceAccount".to_string(),
            api_group: Some(String::new()),
            name: name.to_string(),
            namespace: Some(namespace.to_string()),
        }
    }

    /// Create a new User subject
    pub fn user(name: &str) -> Self {
        Self {
            kind: "User".to_string(),
            api_group: Some("rbac.authorization.k8s.io".to_string()),
            name: name.to_string(),
            namespace: None,
        }
    }

    /// Create a new Group subject
    pub fn group(name: &str) -> Self {
        Self {
            kind: "Group".to_string(),
            api_group: Some("rbac.authorization.k8s.io".to_string()),
            name: name.to_string(),
            namespace: None,
        }
    }
}

/// RoleRef contains information that points to the role being used
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleRef {
    /// API group of the referent (rbac.authorization.k8s.io)
    pub api_group: String,
    /// Kind of the referent (Role or ClusterRole)
    pub kind: String,
    /// Name of the referent
    pub name: String,
}

impl RoleRef {
    /// Create a reference to a Role
    pub fn role(name: &str) -> Self {
        Self {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "Role".to_string(),
            name: name.to_string(),
        }
    }

    /// Create a reference to a ClusterRole
    pub fn cluster_role(name: &str) -> Self {
        Self {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "ClusterRole".to_string(),
            name: name.to_string(),
        }
    }
}
