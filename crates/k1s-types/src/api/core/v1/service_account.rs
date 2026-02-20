//! ServiceAccount resource type

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::common::{LocalObjectReference, ObjectReference};

/// ServiceAccount binds together a name and secrets
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccount {
    #[serde(default = "ServiceAccount::api_version")]
    pub api_version: String,
    #[serde(default = "ServiceAccount::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// Secrets is the list of secrets allowed to be used by pods running using this ServiceAccount
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<ObjectReference>,
    /// ImagePullSecrets is a list of references to secrets for pulling images
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_pull_secrets: Vec<LocalObjectReference>,
    /// AutomountServiceAccountToken indicates whether pods should auto-mount the token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automount_service_account_token: Option<bool>,
}

impl ServiceAccount {
    fn api_version() -> String {
        "v1".to_string()
    }

    fn kind() -> String {
        "ServiceAccount".to_string()
    }

    /// Create a new ServiceAccount with the given name
    pub fn new(name: &str, namespace: &str) -> Self {
        Self {
            api_version: "v1".to_string(),
            kind: "ServiceAccount".to_string(),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

impl Resource for ServiceAccount {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "ServiceAccount";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "serviceaccounts";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}
