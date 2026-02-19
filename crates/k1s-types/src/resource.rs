//! Resource trait and common types

use crate::meta::{ListMeta, ObjectMeta, TypeMeta};
use serde::{de::DeserializeOwned, Serialize};

/// Scope of a Kubernetes resource
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceScope {
    /// Cluster-scoped resources (e.g., Node, Namespace)
    Cluster,
    /// Namespace-scoped resources (e.g., Pod, Deployment)
    Namespaced,
}

/// Trait for Kubernetes resources
pub trait Resource: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {
    /// API version (e.g., "v1", "apps/v1")
    const API_VERSION: &'static str;

    /// Kind (e.g., "Pod", "Deployment")
    const KIND: &'static str;

    /// Plural name for API paths (e.g., "pods", "deployments")
    const PLURAL: &'static str;

    /// Resource scope
    const SCOPE: ResourceScope;

    /// Get the type metadata
    fn type_meta(&self) -> TypeMeta {
        TypeMeta::new(Self::API_VERSION, Self::KIND)
    }

    /// Get object metadata
    fn metadata(&self) -> &ObjectMeta;

    /// Get mutable object metadata
    fn metadata_mut(&mut self) -> &mut ObjectMeta;

    /// Get the resource name
    fn name(&self) -> &str {
        &self.metadata().name
    }

    /// Get the resource namespace (if namespaced)
    fn namespace(&self) -> Option<&str> {
        self.metadata().namespace.as_deref()
    }

    /// Get effective namespace, defaulting to "default"
    fn effective_namespace(&self) -> &str {
        self.metadata().effective_namespace()
    }

    /// Get the resource UID
    fn uid(&self) -> &str {
        &self.metadata().uid
    }

    /// Get the resource version
    fn resource_version(&self) -> &str {
        &self.metadata().resource_version
    }

    /// Storage key for this resource
    fn storage_key(&self) -> String {
        match Self::SCOPE {
            ResourceScope::Cluster => {
                format!("/{}/{}/{}", Self::API_VERSION, Self::PLURAL, self.name())
            }
            ResourceScope::Namespaced => {
                format!(
                    "/{}/{}/{}/{}",
                    Self::API_VERSION,
                    Self::PLURAL,
                    self.effective_namespace(),
                    self.name()
                )
            }
        }
    }

    /// Storage prefix for listing resources
    fn list_prefix(namespace: Option<&str>) -> String {
        match (Self::SCOPE, namespace) {
            (ResourceScope::Cluster, _) => format!("/{}/{}/", Self::API_VERSION, Self::PLURAL),
            (ResourceScope::Namespaced, Some(ns)) => {
                format!("/{}/{}/{}/", Self::API_VERSION, Self::PLURAL, ns)
            }
            (ResourceScope::Namespaced, None) => format!("/{}/{}/", Self::API_VERSION, Self::PLURAL),
        }
    }
}

/// List of resources
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceList<T> {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    pub metadata: ListMeta,
    pub items: Vec<T>,
}

impl<T: Resource> ResourceList<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            type_meta: TypeMeta::new(T::API_VERSION, &format!("{}List", T::KIND)),
            metadata: ListMeta::default(),
            items,
        }
    }

    pub fn with_resource_version(mut self, rv: &str) -> Self {
        self.metadata.resource_version = rv.to_string();
        self
    }
}

impl<T> Default for ResourceList<T> {
    fn default() -> Self {
        Self {
            type_meta: TypeMeta::default(),
            metadata: ListMeta::default(),
            items: Vec::new(),
        }
    }
}
