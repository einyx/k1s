//! Secret resource types

use crate::meta::{ObjectMeta, TypeMeta};
use crate::resource::{Resource, ResourceScope};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Secret holds secret data of a certain type
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Secret {
    #[serde(flatten)]
    pub type_meta: TypeMeta,
    #[serde(default)]
    pub metadata: ObjectMeta,

    /// Data contains the secret data (base64 encoded)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub data: BTreeMap<String, String>,

    /// StringData allows specifying secret data as strings
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub string_data: BTreeMap<String, String>,

    /// Type is used to facilitate programmatic handling
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<SecretType>,

    /// Immutable, if set, prevents data from being updated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immutable: Option<bool>,
}

impl Resource for Secret {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Secret";
    const PLURAL: &'static str = "secrets";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

impl Secret {
    pub fn new(name: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Secret"),
            metadata: ObjectMeta::new(name),
            data: BTreeMap::new(),
            string_data: BTreeMap::new(),
            r#type: Some(SecretType::Opaque),
            immutable: None,
        }
    }

    pub fn namespaced(name: &str, namespace: &str) -> Self {
        Self {
            type_meta: TypeMeta::new("v1", "Secret"),
            metadata: ObjectMeta::namespaced(name, namespace),
            data: BTreeMap::new(),
            string_data: BTreeMap::new(),
            r#type: Some(SecretType::Opaque),
            immutable: None,
        }
    }

    pub fn with_data(mut self, key: &str, value: &str) -> Self {
        use base64::{engine::general_purpose::STANDARD, Engine};
        self.data
            .insert(key.to_string(), STANDARD.encode(value.as_bytes()));
        self
    }

    pub fn with_string_data(mut self, key: &str, value: &str) -> Self {
        self.string_data.insert(key.to_string(), value.to_string());
        self
    }
}

/// Type of secret
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum SecretType {
    #[default]
    #[serde(rename = "Opaque")]
    Opaque,
    #[serde(rename = "kubernetes.io/service-account-token")]
    ServiceAccountToken,
    #[serde(rename = "kubernetes.io/dockercfg")]
    DockerCfg,
    #[serde(rename = "kubernetes.io/dockerconfigjson")]
    DockerConfigJson,
    #[serde(rename = "kubernetes.io/basic-auth")]
    BasicAuth,
    #[serde(rename = "kubernetes.io/ssh-auth")]
    SshAuth,
    #[serde(rename = "kubernetes.io/tls")]
    Tls,
    #[serde(rename = "bootstrap.kubernetes.io/token")]
    BootstrapToken,
}
