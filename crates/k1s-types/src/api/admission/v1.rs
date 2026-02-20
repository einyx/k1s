//! Admission v1 types

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::meta::ObjectMeta;

/// AdmissionReview describes an admission review request/response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionReview {
    #[serde(default = "AdmissionReview::api_version")]
    pub api_version: String,
    #[serde(default = "AdmissionReview::kind")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<AdmissionRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<AdmissionResponse>,
}

impl AdmissionReview {
    fn api_version() -> String {
        "admission.k8s.io/v1".to_string()
    }

    fn kind() -> String {
        "AdmissionReview".to_string()
    }

    pub fn new_request(request: AdmissionRequest) -> Self {
        Self {
            api_version: Self::api_version(),
            kind: Self::kind(),
            request: Some(request),
            response: None,
        }
    }

    pub fn new_response(response: AdmissionResponse) -> Self {
        Self {
            api_version: Self::api_version(),
            kind: Self::kind(),
            request: None,
            response: Some(response),
        }
    }
}

/// AdmissionRequest describes the admission control request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionRequest {
    /// UID is a unique identifier for the individual request/response
    pub uid: String,

    /// Kind is the fully-qualified type of object being submitted
    pub kind: GroupVersionKind,

    /// Resource is the fully-qualified resource being requested
    pub resource: GroupVersionResource,

    /// SubResource is the subresource being requested, if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_resource: Option<String>,

    /// RequestKind is the fully-qualified type of the original API request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_kind: Option<GroupVersionKind>,

    /// RequestResource is the fully-qualified resource of the original API request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_resource: Option<GroupVersionResource>,

    /// Name is the name of the object as presented in the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Namespace is the namespace associated with the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Operation is the operation being performed
    pub operation: Operation,

    /// UserInfo contains information about the requesting user
    pub user_info: UserInfo,

    /// Object is the object from the incoming request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<serde_json::Value>,

    /// OldObject is the existing object (only for UPDATE operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_object: Option<serde_json::Value>,

    /// DryRun indicates that modifications will definitely not be persisted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,

    /// Options contains the options for the operation being performed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
}

/// AdmissionResponse describes the admission control response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdmissionResponse {
    /// UID is the identifier for the individual request/response
    pub uid: String,

    /// Allowed indicates whether or not the admission request was permitted
    pub allowed: bool,

    /// Status contains extra details into why an admission request was denied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,

    /// Patch contains the actual patch (for mutating webhooks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>, // base64-encoded JSON patch

    /// PatchType is the type of patch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_type: Option<PatchType>,

    /// AuditAnnotations is an unstructured key value map set by remote admission controller
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_annotations: Option<BTreeMap<String, String>>,

    /// Warnings is a list of warning messages to return to the requesting API client
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

impl AdmissionResponse {
    /// Create a response that allows the request
    pub fn allow(uid: String) -> Self {
        Self {
            uid,
            allowed: true,
            status: None,
            patch: None,
            patch_type: None,
            audit_annotations: None,
            warnings: None,
        }
    }

    /// Create a response that denies the request
    pub fn deny(uid: String, message: String) -> Self {
        Self {
            uid,
            allowed: false,
            status: Some(Status {
                code: Some(403),
                message: Some(message),
                reason: Some("Forbidden".to_string()),
            }),
            patch: None,
            patch_type: None,
            audit_annotations: None,
            warnings: None,
        }
    }

    /// Create a response with a JSON patch (for mutating webhooks)
    pub fn mutate(uid: String, patch: String) -> Self {
        Self {
            uid,
            allowed: true,
            status: None,
            patch: Some(patch),
            patch_type: Some(PatchType::JsonPatch),
            audit_annotations: None,
            warnings: None,
        }
    }

    /// Add a warning to the response
    pub fn with_warning(mut self, warning: String) -> Self {
        self.warnings.get_or_insert_with(Vec::new).push(warning);
        self
    }

    /// Add an audit annotation
    pub fn with_audit_annotation(mut self, key: String, value: String) -> Self {
        self.audit_annotations
            .get_or_insert_with(BTreeMap::new)
            .insert(key, value);
        self
    }
}

/// GroupVersionKind unambiguously identifies a kind
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupVersionKind {
    pub group: String,
    pub version: String,
    pub kind: String,
}

/// GroupVersionResource unambiguously identifies a resource
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GroupVersionResource {
    pub group: String,
    pub version: String,
    pub resource: String,
}

/// Operation is the type of resource operation being checked for admission control
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Operation {
    Create,
    Update,
    Delete,
    Connect,
}

/// UserInfo holds the information about the user making the request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    /// Username is the name that uniquely identifies this user among all active users
    pub username: String,

    /// UID is a unique value that identifies this user across time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,

    /// Groups are the groups this user is a part of
    #[serde(default)]
    pub groups: Vec<String>,

    /// Extra holds additional information provided by the authenticator
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<BTreeMap<String, Vec<String>>>,
}

/// Status contains details for an API request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// HTTP status code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,

    /// Human-readable description of the status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Machine-readable description of why this operation is in the current state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// PatchType is the type of patch being used
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatchType {
    #[serde(rename = "JSONPatch")]
    JsonPatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admission_response_allow() {
        let response = AdmissionResponse::allow("test-uid".to_string());
        assert!(response.allowed);
        assert_eq!(response.uid, "test-uid");
        assert!(response.status.is_none());
    }

    #[test]
    fn test_admission_response_deny() {
        let response = AdmissionResponse::deny(
            "test-uid".to_string(),
            "Pod name too long".to_string(),
        );
        assert!(!response.allowed);
        assert_eq!(response.uid, "test-uid");
        assert!(response.status.is_some());
        assert_eq!(
            response.status.as_ref().unwrap().message,
            Some("Pod name too long".to_string())
        );
    }

    #[test]
    fn test_admission_response_with_warning() {
        let response = AdmissionResponse::allow("test-uid".to_string())
            .with_warning("This image is deprecated".to_string());
        assert!(response.allowed);
        assert_eq!(response.warnings.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_admission_review_serialization() {
        let review = AdmissionReview::new_response(AdmissionResponse::allow(
            "test-uid".to_string(),
        ));
        let json = serde_json::to_string(&review).unwrap();
        assert!(json.contains("admission.k8s.io/v1"));
        assert!(json.contains("AdmissionReview"));
    }
}
