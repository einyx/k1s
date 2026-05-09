//! CronJob controller
//!
//! Schedules Jobs based on cron expressions.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::batch_v1::{ConcurrencyPolicy, CronJob, Job, ObjectReference};
use k1s_types::{ObjectMeta, OwnerReference};
use tracing::{debug, info, warn};

use crate::{Controller, ControllerResult};

pub struct CronJobController {
    storage: Arc<SledBackend>,
}

impl CronJobController {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self { storage }
    }

    /// Check if a job is owned by the given CronJob
    fn is_owned_by(job: &Job, cronjob: &CronJob) -> bool {
        job.metadata.owner_references.iter().any(|owner| {
            owner.kind == "CronJob"
                && owner.name == cronjob.metadata.name
                && owner.uid == cronjob.metadata.uid
        })
    }

    /// Check if job is still active (not completed or failed)
    fn job_is_active(job: &Job) -> bool {
        if job.metadata.deletion_timestamp.is_some() {
            return false;
        }
        job.status
            .as_ref()
            .map(|s| {
                !s.conditions.iter().any(|c| {
                    (c.r#type == "Complete" || c.r#type == "Failed") && c.status == "True"
                })
            })
            .unwrap_or(true)
    }

    /// Create a Job from CronJob template
    fn create_job_from_template(cronjob: &CronJob) -> Job {
        let spec = cronjob.spec.as_ref().expect("CronJob must have spec");
        let template = &spec.job_template;
        let namespace = cronjob
            .metadata
            .namespace
            .as_deref()
            .unwrap_or("default")
            .to_string();

        // Generate unique job name with timestamp
        let timestamp = Utc::now().format("%Y%m%d%H%M");
        let job_name = format!("{}-{}", cronjob.metadata.name, timestamp);

        // Create owner reference
        let owner_ref = OwnerReference {
            api_version: "batch/v1".to_string(),
            kind: "CronJob".to_string(),
            name: cronjob.metadata.name.clone(),
            uid: cronjob.metadata.uid.clone(),
            controller: Some(true),
            block_owner_deletion: Some(true),
        };

        // Get labels from template
        let mut labels = template.metadata.labels.clone();
        labels.insert("cronjob-name".to_string(), cronjob.metadata.name.clone());

        Job {
            api_version: "batch/v1".to_string(),
            kind: "Job".to_string(),
            metadata: ObjectMeta {
                name: job_name,
                namespace: Some(namespace),
                labels,
                annotations: template.metadata.annotations.clone(),
                owner_references: vec![owner_ref],
                ..Default::default()
            },
            spec: template.spec.clone(),
            status: None,
        }
    }

    /// Parse cron expression and check if it should run now
    /// Simple cron parser for: minute hour day month weekday
    fn should_run_now(schedule: &str, last_schedule: Option<chrono::DateTime<Utc>>) -> bool {
        let now = Utc::now();

        // Don't run more than once per minute
        if let Some(last) = last_schedule {
            if (now - last).num_seconds() < 60 {
                return false;
            }
        }

        // Parse cron expression (simplified)
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        if parts.len() != 5 {
            warn!("Invalid cron expression: {}", schedule);
            return false;
        }

        let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);
        let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
        let day = now.format("%d").to_string().parse::<u32>().unwrap_or(1);
        let month = now.format("%m").to_string().parse::<u32>().unwrap_or(1);
        let weekday = now.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1-7 (Mon-Sun)

        let matches_field = |field: &str, value: u32| -> bool {
            if field == "*" {
                return true;
            }
            // Handle */N syntax
            if let Some(step) = field.strip_prefix("*/") {
                if let Ok(n) = step.parse::<u32>() {
                    return n > 0 && value % n == 0;
                }
            }
            // Handle ranges like 1-5
            if field.contains('-') {
                let range: Vec<&str> = field.split('-').collect();
                if range.len() == 2 {
                    if let (Ok(start), Ok(end)) = (range[0].parse::<u32>(), range[1].parse::<u32>()) {
                        return value >= start && value <= end;
                    }
                }
            }
            // Handle lists like 1,3,5
            if field.contains(',') {
                return field
                    .split(',')
                    .any(|v| v.parse::<u32>().map(|n| n == value).unwrap_or(false));
            }
            // Simple number match
            field.parse::<u32>().map(|n| n == value).unwrap_or(false)
        };

        matches_field(parts[0], minute)
            && matches_field(parts[1], hour)
            && matches_field(parts[2], day)
            && matches_field(parts[3], month)
            && matches_field(parts[4], weekday)
    }
}

#[async_trait]
impl Controller for CronJobController {
    fn name(&self) -> &str {
        "cronjob"
    }

    async fn reconcile(&self) -> ControllerResult<()> {
        let cronjob_store = ResourceStore::<CronJob>::new(self.storage.clone());
        let job_store = ResourceStore::<Job>::new(self.storage.clone());

        let cronjobs = cronjob_store.list(None).await?;

        for cronjob in cronjobs {
            let namespace = cronjob.metadata.namespace.as_deref().unwrap_or("default");
            let spec = match &cronjob.spec {
                Some(s) => s,
                None => continue,
            };

            // Skip suspended cronjobs
            if spec.suspend {
                continue;
            }

            // Find jobs owned by this cronjob
            let all_jobs = job_store.list(Some(namespace)).await?;
            let owned_jobs: Vec<&Job> = all_jobs
                .iter()
                .filter(|j| Self::is_owned_by(j, &cronjob))
                .collect();

            // Get active jobs
            let active_jobs: Vec<&Job> = owned_jobs
                .iter()
                .filter(|j| Self::job_is_active(j))
                .copied()
                .collect();

            debug!(
                "CronJob {}/{}: schedule={}, active_jobs={}",
                namespace,
                cronjob.metadata.name,
                spec.schedule,
                active_jobs.len()
            );

            // Check concurrency policy
            let concurrency_policy = spec
                .concurrency_policy
                .as_ref()
                .unwrap_or(&ConcurrencyPolicy::Allow);

            let can_create = match concurrency_policy {
                ConcurrencyPolicy::Allow => true,
                ConcurrencyPolicy::Forbid => active_jobs.is_empty(),
                ConcurrencyPolicy::Replace => {
                    // Delete active jobs first
                    for job in &active_jobs {
                        info!(
                            "Replacing active Job {}/{} for CronJob {}",
                            namespace, job.metadata.name, cronjob.metadata.name
                        );
                        if let Err(e) = job_store.delete(Some(namespace), &job.metadata.name).await
                        {
                            warn!("Failed to delete Job {}/{}: {}", namespace, job.metadata.name, e);
                        }
                    }
                    true
                }
            };

            // Check if we should create a new job
            let last_schedule = cronjob.status.as_ref().and_then(|s| s.last_schedule_time);

            if can_create && Self::should_run_now(&spec.schedule, last_schedule) {
                info!(
                    "Creating Job for CronJob {}/{} (schedule: {})",
                    namespace, cronjob.metadata.name, spec.schedule
                );

                let job = Self::create_job_from_template(&cronjob);
                match job_store.create(job).await {
                    Ok(created) => {
                        info!(
                            "Created Job {}/{} for CronJob {}",
                            namespace, created.metadata.name, cronjob.metadata.name
                        );

                        // Update cronjob status
                        let mut updated_cronjob = cronjob.clone();
                        let mut status = updated_cronjob.status.clone().unwrap_or_default();
                        status.last_schedule_time = Some(Utc::now());
                        status.active.push(ObjectReference {
                            api_version: Some("batch/v1".to_string()),
                            kind: Some("Job".to_string()),
                            name: Some(created.metadata.name.clone()),
                            namespace: Some(namespace.to_string()),
                            uid: Some(created.metadata.uid.clone()),
                            ..Default::default()
                        });
                        updated_cronjob.status = Some(status);

                        if let Err(e) = cronjob_store.update(updated_cronjob).await {
                            warn!(
                                "Failed to update CronJob {}/{} status: {}",
                                namespace, cronjob.metadata.name, e
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to create Job for CronJob {}/{}: {}",
                            namespace, cronjob.metadata.name, e
                        );
                    }
                }
            }

            // Clean up old jobs based on history limits
            let successful_limit = spec.successful_jobs_history_limit.unwrap_or(3);
            let failed_limit = spec.failed_jobs_history_limit.unwrap_or(1);

            let mut successful_jobs: Vec<&Job> = owned_jobs
                .iter()
                .filter(|j| {
                    j.status.as_ref().is_some_and(|s| {
                        s.conditions
                            .iter()
                            .any(|c| c.r#type == "Complete" && c.status == "True")
                    })
                })
                .copied()
                .collect();

            let mut failed_jobs: Vec<&Job> = owned_jobs
                .iter()
                .filter(|j| {
                    j.status.as_ref().is_some_and(|s| {
                        s.conditions
                            .iter()
                            .any(|c| c.r#type == "Failed" && c.status == "True")
                    })
                })
                .copied()
                .collect();

            // Sort by completion time (oldest first)
            successful_jobs.sort_by(|a, b| {
                let a_time = a.status.as_ref().and_then(|s| s.completion_time);
                let b_time = b.status.as_ref().and_then(|s| s.completion_time);
                a_time.cmp(&b_time)
            });

            failed_jobs.sort_by(|a, b| {
                let a_time = a.status.as_ref().and_then(|s| s.completion_time);
                let b_time = b.status.as_ref().and_then(|s| s.completion_time);
                a_time.cmp(&b_time)
            });

            // Delete excess successful jobs
            if successful_jobs.len() > successful_limit as usize {
                let to_delete = successful_jobs.len() - successful_limit as usize;
                for job in successful_jobs.iter().take(to_delete) {
                    debug!(
                        "Deleting old successful Job {}/{} for CronJob {}",
                        namespace, job.metadata.name, cronjob.metadata.name
                    );
                    if let Err(e) = job_store.delete(Some(namespace), &job.metadata.name).await {
                        warn!("Failed to delete Job {}/{}: {}", namespace, job.metadata.name, e);
                    }
                }
            }

            // Delete excess failed jobs
            if failed_jobs.len() > failed_limit as usize {
                let to_delete = failed_jobs.len() - failed_limit as usize;
                for job in failed_jobs.iter().take(to_delete) {
                    debug!(
                        "Deleting old failed Job {}/{} for CronJob {}",
                        namespace, job.metadata.name, cronjob.metadata.name
                    );
                    if let Err(e) = job_store.delete(Some(namespace), &job.metadata.name).await {
                        warn!("Failed to delete Job {}/{}: {}", namespace, job.metadata.name, e);
                    }
                }
            }

            // Update active job references in status
            let current_active: Vec<ObjectReference> = active_jobs
                .iter()
                .map(|j| ObjectReference {
                    api_version: Some("batch/v1".to_string()),
                    kind: Some("Job".to_string()),
                    name: Some(j.metadata.name.clone()),
                    namespace: Some(namespace.to_string()),
                    uid: Some(j.metadata.uid.clone()),
                    ..Default::default()
                })
                .collect();

            let status_active = cronjob
                .status
                .as_ref()
                .map(|s| s.active.clone())
                .unwrap_or_default();

            if status_active.len() != current_active.len() {
                let mut updated_cronjob = cronjob.clone();
                let mut status = updated_cronjob.status.clone().unwrap_or_default();
                status.active = current_active;
                updated_cronjob.status = Some(status);

                if let Err(e) = cronjob_store.update(updated_cronjob).await {
                    warn!(
                        "Failed to update CronJob {}/{} status: {}",
                        namespace, cronjob.metadata.name, e
                    );
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_every_minute() {
        // "* * * * *" should always match
        assert!(CronJobController::should_run_now("* * * * *", None));
    }

    #[test]
    fn test_cron_rate_limiting() {
        // Should not run if last schedule was less than 60 seconds ago
        let recent = Utc::now() - chrono::Duration::seconds(30);
        assert!(!CronJobController::should_run_now("* * * * *", Some(recent)));
    }

    #[test]
    fn test_cron_step() {
        // Test */5 syntax
        let now = Utc::now();
        let minute = now.format("%M").to_string().parse::<u32>().unwrap_or(0);

        if minute % 5 == 0 {
            assert!(CronJobController::should_run_now("*/5 * * * *", None));
        }
    }
}
