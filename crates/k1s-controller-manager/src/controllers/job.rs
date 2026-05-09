//! Job controller
//!
//! Manages batch jobs by creating pods and tracking completion/failure status.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{
    batch_v1::{condition_type, Job, JobCondition, JobStatus},
    ObjectMeta, OwnerReference, Pod, PodPhase, RestartPolicy, TypeMeta,
};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct JobController {
    storage: Arc<SledBackend>,
}

impl JobController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a pod is owned by the given Job
    fn is_owned_by(pod: &Pod, job: &Job) -> bool {
        pod.metadata.owner_references.iter().any(|owner| {
            owner.kind == "Job"
                && owner.name == job.metadata.name
                && owner.uid == job.metadata.uid
        })
    }

    /// Create a pod from Job template
    fn create_pod_from_template(job: &Job, index: Option<i32>) -> Pod {
        let spec = job.spec.as_ref().expect("Job must have spec");
        let template = &spec.template;
        let namespace = job
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // Generate unique pod name
        let pod_name = match index {
            Some(i) => format!("{}-{}-{}", job.metadata.name, i, generate_suffix()),
            None => format!("{}-{}", job.metadata.name, generate_suffix()),
        };

        // Get labels from template
        let mut labels = template.metadata.labels.clone();
        // Add job-name label for selection
        labels.insert("job-name".to_string(), job.metadata.name.clone());

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "batch/v1".to_string(),
            kind: "Job".to_string(),
            name: job.metadata.name.clone(),
            uid: job.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        // Set restart policy to Never if not specified (Jobs don't restart by default)
        let mut pod_spec = template.spec.clone().unwrap_or_default();
        if pod_spec.restart_policy.is_none() {
            pod_spec.restart_policy = Some(RestartPolicy::Never);
        }

        Pod {
            type_meta: TypeMeta::new("v1", "Pod"),
            metadata: ObjectMeta {
                name: pod_name,
                namespace: Some(namespace),
                labels,
                annotations: template.metadata.annotations.clone(),
                owner_references: vec![owner_ref],
                ..Default::default()
            },
            spec: Some(pod_spec),
            status: None,
        }
    }

    /// Check if a pod has succeeded
    fn pod_succeeded(pod: &Pod) -> bool {
        pod.status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| matches!(p, PodPhase::Succeeded))
            .unwrap_or(false)
    }

    /// Check if a pod has failed
    fn pod_failed(pod: &Pod) -> bool {
        pod.status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| matches!(p, PodPhase::Failed))
            .unwrap_or(false)
    }

    /// Check if a pod is active (running or pending)
    fn pod_is_active(pod: &Pod) -> bool {
        if pod.metadata.deletion_timestamp.is_some() {
            return false;
        }
        pod.status
            .as_ref()
            .and_then(|s| s.phase.as_ref())
            .map(|p| matches!(p, PodPhase::Running | PodPhase::Pending))
            .unwrap_or(true) // No status means pending
    }

    /// Check if job is complete
    fn is_job_complete(job: &Job) -> bool {
        job.status
            .as_ref()
            .map(|s| {
                s.conditions
                    .iter()
                    .any(|c| c.r#type == condition_type::COMPLETE && c.status == "True")
            })
            .unwrap_or(false)
    }

    /// Check if job is failed
    fn is_job_failed(job: &Job) -> bool {
        job.status
            .as_ref()
            .map(|s| {
                s.conditions
                    .iter()
                    .any(|c| c.r#type == condition_type::FAILED && c.status == "True")
            })
            .unwrap_or(false)
    }
}

#[async_trait]
impl Controller for JobController {
    fn name(&self) -> &str {
        "job"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let job_store = ResourceStore::<Job>::new(self.storage.clone());
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        let jobs = job_store.list(None).await?;

        for job in jobs {
            // Skip completed or failed jobs
            if Self::is_job_complete(&job) || Self::is_job_failed(&job) {
                // Handle TTL cleanup
                if let Some(ttl) = job.spec.as_ref().and_then(|s| s.ttl_seconds_after_finished) {
                    if let Some(completion_time) =
                        job.status.as_ref().and_then(|s| s.completion_time)
                    {
                        let elapsed = Utc::now() - completion_time;
                        if elapsed.num_seconds() > ttl as i64 {
                            let namespace = job.metadata.namespace.as_deref().unwrap_or("default");
                            info!(
                                "Deleting Job {}/{} after TTL expiry",
                                namespace, job.metadata.name
                            );
                            if let Err(e) =
                                job_store.delete(Some(namespace), &job.metadata.name).await
                            {
                                warn!("Failed to delete Job {}/{}: {}", namespace, job.metadata.name, e);
                            }
                        }
                    }
                }
                continue;
            }

            // Skip suspended jobs
            if job.spec.as_ref().map(|s| s.suspend).unwrap_or(false) {
                continue;
            }

            let namespace = job.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &job.spec {
                Some(s) => s,
                None => continue,
            };

            let parallelism = spec.parallelism.unwrap_or(1);
            let completions = spec.completions.unwrap_or(1);
            let backoff_limit = spec.backoff_limit.unwrap_or(6);

            // Find pods owned by this job
            let all_pods = pod_store.list(Some(namespace)).await?;
            let owned_pods: Vec<&Pod> = all_pods.iter().filter(|p| Self::is_owned_by(p, &job)).collect();

            // Count pod states
            let succeeded_count = owned_pods.iter().filter(|p| Self::pod_succeeded(p)).count() as i32;
            let failed_count = owned_pods.iter().filter(|p| Self::pod_failed(p)).count() as i32;
            let active_pods: Vec<&&Pod> = owned_pods.iter().filter(|p| Self::pod_is_active(p)).collect();
            let active_count = active_pods.len() as i32;

            debug!(
                "Job {}/{}: parallelism={}, completions={}, active={}, succeeded={}, failed={}",
                namespace, job.metadata.name, parallelism, completions, active_count, succeeded_count, failed_count
            );

            // Check if job should be marked as failed
            if failed_count > backoff_limit {
                info!(
                    "Job {}/{} exceeded backoff limit ({} > {}), marking as failed",
                    namespace, job.metadata.name, failed_count, backoff_limit
                );
                let mut updated_job = job.clone();
                let mut status = updated_job.status.clone().unwrap_or_default();
                status.conditions.push(JobCondition {
                    r#type: condition_type::FAILED.to_string(),
                    status: "True".to_string(),
                    last_transition_time: Some(Utc::now()),
                    reason: Some("BackoffLimitExceeded".to_string()),
                    message: Some(format!(
                        "Job has reached the specified backoff limit ({backoff_limit})"
                    )),
                    ..Default::default()
                });
                updated_job.status = Some(status);
                if let Err(e) = job_store.update(updated_job).await {
                    warn!("Failed to update Job {}/{} status: {}", namespace, job.metadata.name, e);
                }
                continue;
            }

            // Check if job is complete
            if succeeded_count >= completions {
                info!(
                    "Job {}/{} completed ({}/{} succeeded)",
                    namespace, job.metadata.name, succeeded_count, completions
                );
                let mut updated_job = job.clone();
                let mut status = updated_job.status.clone().unwrap_or_default();
                status.completion_time = Some(Utc::now());
                status.conditions.push(JobCondition {
                    r#type: condition_type::COMPLETE.to_string(),
                    status: "True".to_string(),
                    last_transition_time: Some(Utc::now()),
                    reason: Some("Completed".to_string()),
                    message: Some("Job completed successfully".to_string()),
                    ..Default::default()
                });
                status.succeeded = succeeded_count;
                status.active = 0;
                updated_job.status = Some(status);
                if let Err(e) = job_store.update(updated_job).await {
                    warn!("Failed to update Job {}/{} status: {}", namespace, job.metadata.name, e);
                }
                continue;
            }

            // Calculate how many pods we still need to create
            let total_started = succeeded_count + failed_count + active_count;
            let remaining = completions - succeeded_count;
            let to_create = (parallelism - active_count).min(remaining).max(0);

            // Create new pods if needed
            if to_create > 0 && total_started < completions + backoff_limit {
                info!(
                    "Creating {} pods for Job {}/{}",
                    to_create, namespace, job.metadata.name
                );

                for i in 0..to_create {
                    let pod = Self::create_pod_from_template(&job, Some(total_started + i));
                    match pod_store.create(pod).await {
                        Ok(created) => {
                            info!(
                                "Created pod {}/{} for Job {}",
                                namespace, created.metadata.name, job.metadata.name
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create pod for Job {}/{}: {}",
                                namespace, job.metadata.name, e
                            );
                        }
                    }
                }
            }

            // Update job status
            let current_status = job.status.clone().unwrap_or_default();
            if current_status.active != active_count
                || current_status.succeeded != succeeded_count
                || current_status.failed != failed_count
            {
                let mut updated_job = job.clone();
                updated_job.status = Some(JobStatus {
                    active: active_count,
                    succeeded: succeeded_count,
                    failed: failed_count,
                    start_time: current_status
                        .start_time
                        .or_else(|| Some(Utc::now())),
                    conditions: current_status.conditions,
                    ..Default::default()
                });

                if let Err(e) = job_store.update(updated_job).await {
                    warn!("Failed to update Job {}/{} status: {}", namespace, job.metadata.name, e);
                }
            }
        }

        Ok(())
    }
}

/// Generate a random 5-character suffix for pod names
fn generate_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let chars: Vec<char> = "abcdefghijklmnopqrstuvwxyz0123456789".chars().collect();
    let mut suffix = String::new();
    let mut n = now as u64;
    for _ in 0..5 {
        suffix.push(chars[(n % 36) as usize]);
        n /= 36;
    }
    suffix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_suffix() {
        let s1 = generate_suffix();
        let s2 = generate_suffix();
        assert_eq!(s1.len(), 5);
        assert_eq!(s2.len(), 5);
    }
}
