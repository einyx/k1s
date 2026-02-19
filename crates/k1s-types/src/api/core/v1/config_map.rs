//! ConfigMap resource types

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// ConfigMap holds configuration data for pods to consume
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMap {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,

    /// Data contains the configuration data as UTF-8 strings
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, String>,

    /// BinaryData contains the configuration data as binary
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binary_data: BTreeMap<String, String>,

    /// Immutable, if set, prevents data from being updated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immutable: Option<bool>,
}

impl Resource for ConfigMap {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "ConfigMap";
    const PLURAL: &'static str = "configmaps";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl ConfigMap {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "ConfigMap"),
            metadata: ObjectMeta::new(name),
            data: BTreeMap::new(),
            binary_data: BTreeMap::new(),
            immutable: None,
        }
    }

    pub fn namespaced(name: &str, namespace: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "ConfigMap"),
            metadata: ObjectMeta::namespaced(name, namespace),
            data: BTreeMap::new(),
            binary_data: BTreeMap::new(),
            immutable: None,
        }
    }

    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        self.data.insert(key.to_string(), value.to_string());
        self
    }
}
