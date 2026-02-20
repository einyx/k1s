//! Webhook invocation logic for calling external admission webhooks

use std::sync::Arc;
use std::time::Duration;

use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::api::admission::v1::{AdmissionRequest, AdmissionReview, AdmissionResponse};
use k1s_types::api::admissionregistration::v1::{
    ValidatingWebhookConfiguration, MutatingWebhookConfiguration,
    OperationType, FailurePolicy,
};
use reqwest::Client;
use tracing::{debug, error, warn};

/// WebhookInvoker handles calling external admission webhooks
pub struct WebhookInvoker {
    storage: Arc<SledBackend>,
    http_client: Client,
    default_timeout: Duration,
}

impl WebhookInvoker {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(true) // For development; in production, verify certs
            .build()
            .expect("Failed to create HTTP client");

        Self {
            storage,
            http_client,
            default_timeout: Duration::from_secs(10),
        }
    }

    /// Call all matching validating webhooks
    pub async fn validate(&self, request: &AdmissionRequest) -> Result<AdmissionResponse, String> {
        let store = ResourceStore::<ValidatingWebhookConfiguration>::new(self.storage.clone());
        let configs = store.list(None).await.map_err(|e| format!("Failed to list webhook configs: {}", e))?;

        for config in configs {
            for webhook in &config.webhooks {
                // Check if this webhook matches the request
                if !self.webhook_matches_request(&webhook.rules, request) {
                    continue;
                }

                debug!("Calling validating webhook: {}", webhook.name);

                // Build admission review request
                let review = AdmissionReview::new_request(request.clone());

                // Call the webhook
                match self.call_webhook(&webhook.client_config.url, &review, webhook.timeout_seconds).await {
                    Ok(response) => {
                        if !response.allowed {
                            debug!("Webhook {} denied request: {:?}", webhook.name, response.status);
                            return Ok(response);
                        }
                    }
                    Err(e) => {
                        let failure_policy = webhook.failure_policy.as_ref().unwrap_or(&FailurePolicy::Fail);
                        match failure_policy {
                            FailurePolicy::Fail => {
                                error!("Webhook {} failed: {}", webhook.name, e);
                                return Err(format!("Webhook {} failed: {}", webhook.name, e));
                            }
                            FailurePolicy::Ignore => {
                                warn!("Webhook {} failed (ignored): {}", webhook.name, e);
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // All webhooks passed or were ignored
        Ok(AdmissionResponse::allow(request.uid.clone()))
    }

    /// Call all matching mutating webhooks
    pub async fn mutate(&self, request: &AdmissionRequest) -> Result<AdmissionResponse, String> {
        let store = ResourceStore::<MutatingWebhookConfiguration>::new(self.storage.clone());
        let configs = store.list(None).await.map_err(|e| format!("Failed to list webhook configs: {}", e))?;

        let mut combined_patches = Vec::new();
        let mut current_request = request.clone();

        for config in configs {
            for webhook in &config.webhooks {
                // Check if this webhook matches the request
                if !self.webhook_matches_request(&webhook.rules, request) {
                    continue;
                }

                debug!("Calling mutating webhook: {}", webhook.name);

                // Build admission review request
                let review = AdmissionReview::new_request(current_request.clone());

                // Call the webhook
                match self.call_webhook(&webhook.client_config.url, &review, webhook.timeout_seconds).await {
                    Ok(response) => {
                        if !response.allowed {
                            debug!("Mutating webhook {} denied request", webhook.name);
                            return Ok(response);
                        }

                        // Collect patches
                        if let Some(patch) = response.patch {
                            combined_patches.push(patch);
                        }
                    }
                    Err(e) => {
                        let failure_policy = webhook.failure_policy.as_ref().unwrap_or(&FailurePolicy::Fail);
                        match failure_policy {
                            FailurePolicy::Fail => {
                                error!("Mutating webhook {} failed: {}", webhook.name, e);
                                return Err(format!("Webhook {} failed: {}", webhook.name, e));
                            }
                            FailurePolicy::Ignore => {
                                warn!("Mutating webhook {} failed (ignored): {}", webhook.name, e);
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // Return combined response
        if combined_patches.is_empty() {
            Ok(AdmissionResponse::allow(request.uid.clone()))
        } else {
            // Combine all patches (simplified - in production, apply patches sequentially)
            let combined = combined_patches.join(",");
            Ok(AdmissionResponse::mutate(request.uid.clone(), combined))
        }
    }

    /// Call a single webhook endpoint
    async fn call_webhook(
        &self,
        url: &Option<String>,
        review: &AdmissionReview,
        timeout_seconds: Option<i32>,
    ) -> Result<AdmissionResponse, String> {
        let webhook_url = url.as_ref().ok_or("Webhook URL not specified")?;

        let timeout = timeout_seconds
            .map(|s| Duration::from_secs(s as u64))
            .unwrap_or(self.default_timeout);

        let client = self.http_client.clone();

        let response = tokio::time::timeout(
            timeout,
            client.post(webhook_url)
                .json(review)
                .send()
        ).await
            .map_err(|_| "Webhook request timed out".to_string())?
            .map_err(|e| format!("Webhook request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Webhook returned status: {}", response.status()));
        }

        let review_response: AdmissionReview = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse webhook response: {}", e))?;

        review_response.response.ok_or("Webhook response missing".to_string())
    }

    /// Check if webhook rules match the admission request
    fn webhook_matches_request(
        &self,
        rules: &[k1s_types::api::admissionregistration::v1::RuleWithOperations],
        request: &AdmissionRequest,
    ) -> bool {
        for rule in rules {
            // Check operations
            let operation_matches = rule.operations.iter().any(|op| {
                matches!(op, OperationType::All) || match (&request.operation, op) {
                    (k1s_types::api::admission::v1::Operation::Create, OperationType::Create) => true,
                    (k1s_types::api::admission::v1::Operation::Update, OperationType::Update) => true,
                    (k1s_types::api::admission::v1::Operation::Delete, OperationType::Delete) => true,
                    (k1s_types::api::admission::v1::Operation::Connect, OperationType::Connect) => true,
                    _ => false,
                }
            });

            if !operation_matches {
                continue;
            }

            // Check API groups
            let group_matches = rule.rule.api_groups.is_empty()
                || rule.rule.api_groups.contains(&"*".to_string())
                || rule.rule.api_groups.contains(&request.resource.group);

            if !group_matches {
                continue;
            }

            // Check API versions
            let version_matches = rule.rule.api_versions.is_empty()
                || rule.rule.api_versions.contains(&"*".to_string())
                || rule.rule.api_versions.contains(&request.resource.version);

            if !version_matches {
                continue;
            }

            // Check resources
            let resource_matches = rule.rule.resources.is_empty()
                || rule.rule.resources.contains(&"*".to_string())
                || rule.rule.resources.contains(&request.resource.resource);

            if resource_matches {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1s_types::api::admission::v1::{GroupVersionKind, GroupVersionResource, Operation, UserInfo};
    use k1s_types::api::admissionregistration::v1::{RuleWithOperations, Rule};

    #[test]
    fn test_webhook_matches_request() {
        let invoker = WebhookInvoker::new(Arc::new(SledBackend::in_memory().unwrap()));

        let request = AdmissionRequest {
            uid: "test".to_string(),
            kind: GroupVersionKind {
                group: "".to_string(),
                version: "v1".to_string(),
                kind: "Pod".to_string(),
            },
            resource: GroupVersionResource {
                group: "".to_string(),
                version: "v1".to_string(),
                resource: "pods".to_string(),
            },
            sub_resource: None,
            request_kind: None,
            request_resource: None,
            name: Some("test".to_string()),
            namespace: Some("default".to_string()),
            operation: Operation::Create,
            user_info: UserInfo {
                username: "test".to_string(),
                uid: None,
                groups: vec![],
                extra: None,
            },
            object: None,
            old_object: None,
            dry_run: None,
            options: None,
        };

        let rules = vec![RuleWithOperations {
            operations: vec![OperationType::Create],
            rule: Rule {
                api_groups: vec!["".to_string()],
                api_versions: vec!["v1".to_string()],
                resources: vec!["pods".to_string()],
                scope: None,
            },
        }];

        assert!(invoker.webhook_matches_request(&rules, &request));
    }

    #[test]
    fn test_webhook_wildcard_matches() {
        let invoker = WebhookInvoker::new(Arc::new(SledBackend::in_memory().unwrap()));

        let request = AdmissionRequest {
            uid: "test".to_string(),
            kind: GroupVersionKind {
                group: "apps".to_string(),
                version: "v1".to_string(),
                kind: "Deployment".to_string(),
            },
            resource: GroupVersionResource {
                group: "apps".to_string(),
                version: "v1".to_string(),
                resource: "deployments".to_string(),
            },
            sub_resource: None,
            request_kind: None,
            request_resource: None,
            name: Some("test".to_string()),
            namespace: Some("default".to_string()),
            operation: Operation::Update,
            user_info: UserInfo {
                username: "test".to_string(),
                uid: None,
                groups: vec![],
                extra: None,
            },
            object: None,
            old_object: None,
            dry_run: None,
            options: None,
        };

        let rules = vec![RuleWithOperations {
            operations: vec![OperationType::All],
            rule: Rule {
                api_groups: vec!["*".to_string()],
                api_versions: vec!["*".to_string()],
                resources: vec!["*".to_string()],
                scope: None,
            },
        }];

        assert!(invoker.webhook_matches_request(&rules, &request));
    }
}
