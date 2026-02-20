//! Event resource type
//!
//! Events are reports of state changes in the cluster, used for debugging.

use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

/// Event is a report of an event somewhere in the cluster
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(default = "Event::api_version")]
    pub api_version: String,
    #[serde(default = "Event::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    /// The object that this event is about
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub involved_object: Option<ObjectReference>,
    /// A short, machine understandable string that gives the reason for the event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// A human-readable description of the event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The component reporting this event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<EventSource>,
    /// The time at which the event was first recorded
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_timestamp: Option<String>,
    /// The time at which the most recent occurrence of this event was recorded
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_timestamp: Option<String>,
    /// The number of times this event has occurred
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    /// Type of this event (Normal, Warning)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    /// What action was taken/failed regarding the object
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// ID of the controller instance
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting_controller: Option<String>,
    /// ID of the controller instance
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reporting_instance: Option<String>,
}

impl Event {
    fn api_version() -> String {
        "v1".to_string()
    }

    fn kind() -> String {
        "Event".to_string()
    }

    /// Create a new normal event
    pub fn normal(
        namespace: &str,
        name: &str,
        involved_object: ObjectReference,
        reason: &str,
        message: &str,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            api_version: "v1".to_string(),
            kind: "Event".to_string(),
            metadata: ObjectMeta {
                name: name.to_string(),
                namespace: Some(namespace.to_string()),
                ..Default::default()
            },
            involved_object: Some(involved_object),
            reason: Some(reason.to_string()),
            message: Some(message.to_string()),
            first_timestamp: Some(now.clone()),
            last_timestamp: Some(now),
            count: Some(1),
            r#type: Some("Normal".to_string()),
            source: None,
            action: None,
            reporting_controller: None,
            reporting_instance: None,
        }
    }

    /// Create a new warning event
    pub fn warning(
        namespace: &str,
        name: &str,
        involved_object: ObjectReference,
        reason: &str,
        message: &str,
    ) -> Self {
        let mut event = Self::normal(namespace, name, involved_object, reason, message);
        event.r#type = Some("Warning".to_string());
        event
    }

    /// Set the source component
    pub fn with_source(mut self, component: &str, host: Option<&str>) -> Self {
        self.source = Some(EventSource {
            component: Some(component.to_string()),
            host: host.map(String::from),
        });
        self
    }

    /// Increment the event count
    pub fn increment(&mut self) {
        self.count = Some(self.count.unwrap_or(0) + 1);
        self.last_timestamp = Some(chrono::Utc::now().to_rfc3339());
    }
}

impl Resource for Event {
    const API_VERSION: &'static str = "v1";
    const KIND: &'static str = "Event";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "events";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// EventSource contains information for an event
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventSource {
    /// Component from which the event is generated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Node name on which the event is generated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

/// ObjectReference contains enough information to let you inspect or modify the referred object
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_path: Option<String>,
}

impl ObjectReference {
    /// Create a reference from resource metadata
    pub fn from_resource<R: Resource>(resource: &R, api_version: &str, kind: &str) -> Self {
        let meta = resource.metadata();
        Self {
            api_version: Some(api_version.to_string()),
            kind: Some(kind.to_string()),
            name: Some(meta.name.clone()),
            namespace: meta.namespace.clone(),
            uid: Some(meta.uid.clone()),
            resource_version: Some(meta.resource_version.clone()),
            field_path: None,
        }
    }
}
