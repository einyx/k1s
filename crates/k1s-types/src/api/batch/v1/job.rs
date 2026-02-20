//! Job resource type

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::apps::v1::{LabelSelector, PodTemplateSpec};
use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

/// Job represents the configuration of a single job
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    #[serde(default = "Job::api_version")]
    pub api_version: String,
    #[serde(default = "Job::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<JobSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<JobStatus>,
}

impl Job {
    fn api_version() -> String {
        "batch/v1".to_string()
    }

    fn kind() -> String {
        "Job".to_string()
    }
}

impl Resource for Job {
    const API_VERSION: &'static str = "batch/v1";
    const KIND: &'static str = "Job";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "jobs";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// JobSpec describes how the job execution will look like
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    /// Number of parallel pods to run
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallelism: Option<i32>,
    /// Number of successful pods required for completion
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completions: Option<i32>,
    /// Maximum time in seconds before the job is terminated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_deadline_seconds: Option<i64>,
    /// Number of retries before marking this job failed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backoff_limit: Option<i32>,
    /// Selector for pods belonging to this job
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selector: Option<LabelSelector>,
    /// Template for pod creation
    pub template: PodTemplateSpec,
    /// TTL in seconds after finished before cleanup
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds_after_finished: Option<i32>,
    /// Suspend the job
    #[serde(default)]
    pub suspend: bool,
    /// Completion mode (NonIndexed or Indexed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_mode: Option<String>,
}

/// JobStatus represents the current state of a Job
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    /// Current conditions of the job
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<JobCondition>,
    /// Timestamp when the job started
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_time: Option<DateTime<Utc>>,
    /// Timestamp when the job completed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_time: Option<DateTime<Utc>>,
    /// Number of active pods
    #[serde(default)]
    pub active: i32,
    /// Number of succeeded pods
    #[serde(default)]
    pub succeeded: i32,
    /// Number of failed pods
    #[serde(default)]
    pub failed: i32,
    /// Number of pods which are terminating
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminating: Option<i32>,
    /// Completed indexes (for indexed jobs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_indexes: Option<String>,
    /// Number of ready pods
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<i32>,
}

/// JobCondition describes current state of a job
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobCondition {
    pub r#type: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_probe_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Job condition types
pub mod condition_type {
    pub const COMPLETE: &str = "Complete";
    pub const FAILED: &str = "Failed";
    pub const SUSPENDED: &str = "Suspended";
    pub const FAILURE_TARGET: &str = "FailureTarget";
    pub const SUCCESS_CRITERIA_MET: &str = "SuccessCriteriaMet";
}
