//! CronJob resource type

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::meta::ObjectMeta;
use crate::resource::{Resource, ResourceScope};

use super::job::JobSpec;

/// CronJob represents the configuration of a cron job
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    #[serde(default = "CronJob::api_version")]
    pub api_version: String,
    #[serde(default = "CronJob::kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<CronJobSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CronJobStatus>,
}

impl CronJob {
    fn api_version() -> String {
        "batch/v1".to_string()
    }

    fn kind() -> String {
        "CronJob".to_string()
    }
}

impl Resource for CronJob {
    const API_VERSION: &'static str = "batch/v1";
    const KIND: &'static str = "CronJob";
    const SCOPE: ResourceScope = ResourceScope::Namespaced;
    const PLURAL: &'static str = "cronjobs";

    fn metadata(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}

/// CronJobSpec describes how the cron job execution will look like
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CronJobSpec {
    /// Cron schedule (e.g., "*/5 * * * *")
    pub schedule: String,
    /// Timezone for the schedule
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_zone: Option<String>,
    /// Optional deadline in seconds for starting the job
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_deadline_seconds: Option<i64>,
    /// Concurrency policy (Allow, Forbid, Replace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency_policy: Option<ConcurrencyPolicy>,
    /// Whether to suspend the cron job
    #[serde(default)]
    pub suspend: bool,
    /// Template for job creation
    pub job_template: JobTemplateSpec,
    /// Number of successful finished jobs to keep
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successful_jobs_history_limit: Option<i32>,
    /// Number of failed finished jobs to keep
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_jobs_history_limit: Option<i32>,
}

/// ConcurrencyPolicy describes how to handle concurrent jobs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConcurrencyPolicy {
    Allow,
    Forbid,
    Replace,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        ConcurrencyPolicy::Allow
    }
}

/// JobTemplateSpec describes the data a Job should have when created from a template
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobTemplateSpec {
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<JobSpec>,
}

/// CronJobStatus represents the current state of a cron job
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CronJobStatus {
    /// List of currently running jobs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active: Vec<ObjectReference>,
    /// Last time the job was successfully scheduled
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_schedule_time: Option<DateTime<Utc>>,
    /// Last time a job was successfully completed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_time: Option<DateTime<Utc>>,
}

/// ObjectReference contains enough information to let you identify an object
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
}
