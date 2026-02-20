//! Admission registration v1 types

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

/// ValidatingWebhookConfiguration describes the configuration of validating admission webhooks
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatingWebhookConfiguration {
    #[serde(default = "ValidatingWebhookConfiguration::api_version")]
    pub api_version: String,
    #[serde(default = "ValidatingWebhookConfiguration::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub webhooks: Vec<ValidatingWebhook>,
}

impl ValidatingWebhookConfiguration {
    fn api_version() -> String {
        "admissionregistration.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "ValidatingWebhookConfiguration".to_string()
    }
}

impl Resource for ValidatingWebhookConfiguration {
    const API_VERSION: &'static str = "admissionregistration.k8s.io/v1";
    const KIND: &'static str = "ValidatingWebhookConfiguration";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "validatingwebhookconfigurations";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// MutatingWebhookConfiguration describes the configuration of mutating admission webhooks
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutatingWebhookConfiguration {
    #[serde(default = "MutatingWebhookConfiguration::api_version")]
    pub api_version: String,
    #[serde(default = "MutatingWebhookConfiguration::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub webhooks: Vec<MutatingWebhook>,
}

impl MutatingWebhookConfiguration {
    fn api_version() -> String {
        "admissionregistration.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "MutatingWebhookConfiguration".to_string()
    }
}

impl Resource for MutatingWebhookConfiguration {
    const API_VERSION: &'static str = "admissionregistration.k8s.io/v1";
    const KIND: &'static str = "MutatingWebhookConfiguration";
    const SCOPE: ResourceScope = ResourceScope::Cluster;
    const PLURAL: &'static str = "mutatingwebhookconfigurations";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// ValidatingWebhook describes a validating admission webhook
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatingWebhook {
    /// Name is the full-qualified name of the webhook
    pub name: String,

    /// ClientConfig defines how to communicate with the hook
    pub client_config: WebhookClientConfig,

    /// Rules describes what operations on what resources the webhook cares about
    pub rules: Vec<RuleWithOperations>,

    /// FailurePolicy defines how unrecognized errors are handled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_policy: Option<FailurePolicy>,

    /// MatchPolicy defines how the "rules" list is used to match incoming requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_policy: Option<MatchPolicy>,

    /// NamespaceSelector decides whether to run the webhook on an object based on namespace labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_selector: Option<LabelSelector>,

    /// ObjectSelector decides whether to run the webhook based on object labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_selector: Option<LabelSelector>,

    /// SideEffects states whether this webhook has side effects
    pub side_effects: SideEffectClass,

    /// TimeoutSeconds is the timeout for this webhook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,

    /// AdmissionReviewVersions is an ordered list of preferred versions
    pub admission_review_versions: Vec<String>,
}

/// MutatingWebhook describes a mutating admission webhook
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MutatingWebhook {
    /// Name is the full-qualified name of the webhook
    pub name: String,

    /// ClientConfig defines how to communicate with the hook
    pub client_config: WebhookClientConfig,

    /// Rules describes what operations on what resources the webhook cares about
    pub rules: Vec<RuleWithOperations>,

    /// FailurePolicy defines how unrecognized errors are handled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_policy: Option<FailurePolicy>,

    /// MatchPolicy defines how the "rules" list is used to match incoming requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_policy: Option<MatchPolicy>,

    /// NamespaceSelector decides whether to run the webhook on an object based on namespace labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_selector: Option<LabelSelector>,

    /// ObjectSelector decides whether to run the webhook based on object labels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_selector: Option<LabelSelector>,

    /// SideEffects states whether this webhook has side effects
    pub side_effects: SideEffectClass,

    /// TimeoutSeconds is the timeout for this webhook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,

    /// AdmissionReviewVersions is an ordered list of preferred versions
    pub admission_review_versions: Vec<String>,

    /// ReinvocationPolicy indicates whether this webhook should be called multiple times
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reinvocation_policy: Option<ReinvocationPolicy>,
}

/// WebhookClientConfig contains the information to make a TLS connection with the webhook
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebhookClientConfig {
    /// URL gives the location of the webhook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Service is a reference to the service for this webhook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceReference>,

    /// CABundle is a PEM encoded CA bundle which will be used to validate the webhook's server certificate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_bundle: Option<String>, // base64 encoded
}

/// ServiceReference holds a reference to a Service
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReference {
    /// Namespace is the namespace of the service
    pub namespace: String,

    /// Name is the name of the service
    pub name: String,

    /// Path is an optional URL path which will be sent in any request to this service
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Port is the port of the service
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<i32>,
}

/// RuleWithOperations is a tuple of Operations and Resources
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuleWithOperations {
    /// Operations is the operations the admission hook cares about
    pub operations: Vec<OperationType>,

    /// Rule is embedded, it describes other criteria of the rule
    #[serde(flatten)]
    pub rule: Rule,
}

/// Rule describes what resources and what operations the rule applies to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// APIGroups is the API groups the resources belong to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_groups: Vec<String>,

    /// APIVersions is the API versions the resources belong to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_versions: Vec<String>,

    /// Resources is a list of resources this rule applies to
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<String>,

    /// Scope specifies the scope of this rule
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopeType>,
}

/// OperationType specifies an operation for a request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum OperationType {
    Create,
    Update,
    Delete,
    Connect,
    #[serde(rename = "*")]
    All,
}

/// ScopeType specifies a scope for a rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScopeType {
    #[serde(rename = "Cluster")]
    Cluster,
    #[serde(rename = "Namespaced")]
    Namespaced,
    #[serde(rename = "*")]
    All,
}

/// FailurePolicy describes how unrecognized errors from the admission endpoint are handled
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailurePolicy {
    /// Fail means that an error calling the webhook causes the admission to fail
    Fail,
    /// Ignore means that an error calling the webhook is ignored
    Ignore,
}

impl Default for FailurePolicy {
    fn default() -> Self {
        Self::Fail
    }
}

/// MatchPolicy defines how the rules list is used to match incoming requests
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MatchPolicy {
    /// Exact means that the operation matches the exact rules specified
    Exact,
    /// Equivalent means that an operation matches a rule if it modifies the same resource
    Equivalent,
}

impl Default for MatchPolicy {
    fn default() -> Self {
        Self::Equivalent
    }
}

/// SideEffectClass denotes the side effects of a webhook
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SideEffectClass {
    /// None means the webhook has no side effects
    None,
    /// NoneOnDryRun means the webhook has no side effects when dryRun is true
    NoneOnDryRun,
    /// Some means the webhook may have side effects
    Some,
    /// Unknown means the side effects of the webhook are unknown
    Unknown,
}

/// ReinvocationPolicy describes whether a webhook can be called multiple times
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReinvocationPolicy {
    /// Never means the webhook will not be called more than once per admission evaluation
    Never,
    /// IfNeeded means the webhook may be called again as part of the admission evaluation
    IfNeeded,
}

/// LabelSelector is used to select resources based on labels
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelector {
    /// MatchLabels is a map of {key,value} pairs
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub match_labels: BTreeMap<String, String>,

    /// MatchExpressions is a list of label selector requirements
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub match_expressions: Vec<LabelSelectorRequirement>,
}

/// LabelSelectorRequirement is a selector that contains values, a key, and an operator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelSelectorRequirement {
    /// Key is the label key that the selector applies to
    pub key: String,

    /// Operator represents a key's relationship to a set of values
    pub operator: LabelSelectorOperator,

    /// Values is an array of string values
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// LabelSelectorOperator is the set of operators that can be used in a selector requirement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validating_webhook_config() {
        let config = ValidatingWebhookConfiguration {
            api_version: "admissionregistration.k8s.io/v1".to_string(),
            kind: "ValidatingWebhookConfiguration".to_string(),
            metadata: ObjectMeta {
                name: "test-webhook".to_string(),
                ..Default::default()
            },
            webhooks: vec![],
        };

        assert_eq!(config.metadata.name, "test-webhook");
    }

    #[test]
    fn test_failure_policy_default() {
        let policy = FailurePolicy::default();
        assert_eq!(policy, FailurePolicy::Fail);
    }
}
