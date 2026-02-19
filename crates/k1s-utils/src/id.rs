//! Resource identification utilities

use serde::{Deserialize, Serialize};

/// Fully qualified resource identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    pub api_version: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

impl ResourceId {
    pub fn namespaced(api_version: &str, kind: &str, namespace: &str, name: &str) -> Self {
        Self {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            namespace: Some(namespace.to_string()),
            name: name.to_string(),
        }
    }

    pub fn cluster_scoped(api_version: &str, kind: &str, name: &str) -> Self {
        Self {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            namespace: None,
            name: name.to_string(),
        }
    }

    /// Storage key for this resource
    pub fn storage_key(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("/{}/{}/{}/{}", self.api_version, self.kind, ns, self.name),
            None => format!("/{}/{}/{}", self.api_version, self.kind, self.name),
        }
    }

    /// Storage prefix for listing resources of this type
    pub fn list_prefix(api_version: &str, kind: &str, namespace: Option<&str>) -> String {
        match namespace {
            Some(ns) => format!("/{}/{}/{}/", api_version, kind, ns),
            None => format!("/{}/{}/", api_version, kind),
        }
    }
}
