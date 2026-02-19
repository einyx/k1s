//! Pod scheduler with filter/score plugins
//!
//! Watches for unscheduled pods and assigns them to suitable nodes
//! based on resource requirements, constraints, and scoring.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{Node, Pod};
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("No nodes available")]
    NoNodesAvailable,

    #[error("No feasible nodes for pod")]
    NoFeasibleNodes,

    #[error("Storage error: {0}")]
    Storage(#[from] k1s_storage::StorageError),
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub schedule_interval: Duration,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            schedule_interval: Duration::from_secs(1),
        }
    }
}

/// Filter plugin trait
#[async_trait]
pub trait FilterPlugin: Send + Sync {
    fn name(&self) -> &str;
    async fn filter(&self, pod: &Pod, node: &Node) -> bool;
}

/// Score plugin trait
#[async_trait]
pub trait ScorePlugin: Send + Sync {
    fn name(&self) -> &str;
    async fn score(&self, pod: &Pod, node: &Node) -> i64;
}

/// The pod scheduler
pub struct Scheduler {
    config: SchedulerConfig,
    storage: Arc<SledBackend>,
    filter_plugins: Vec<Box<dyn FilterPlugin>>,
    score_plugins: Vec<Box<dyn ScorePlugin>>,
}

impl Scheduler {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self::with_config(storage, SchedulerConfig::default())
    }

    pub fn with_config(storage: Arc<SledBackend>, config: SchedulerConfig) -> Self {
        Self {
            config,
            storage,
            filter_plugins: vec![
                Box::new(NodeReadyFilter),
                Box::new(ResourceFitFilter),
                Box::new(NodeSelectorFilter),
                Box::new(TaintTolerationFilter),
            ],
            score_plugins: vec![
                Box::new(LeastRequestedScore),
                Box::new(BalancedResourceScore),
            ],
        }
    }

    /// Run the scheduler loop
    pub async fn run(&self) -> SchedulerResult<()> {
        info!("Starting scheduler");

        let mut ticker = interval(self.config.schedule_interval);

        loop {
            ticker.tick().await;

            if let Err(e) = self.schedule_pending_pods().await {
                error!("Scheduling cycle failed: {}", e);
            }
        }
    }

    /// Schedule all pending pods
    async fn schedule_pending_pods(&self) -> SchedulerResult<()> {
        let pod_store = ResourceStore::<Pod>::new(self.storage.clone());

        // List all pods
        let pods = pod_store.list(None).await?;

        for pod in pods {
            // Skip pods that already have a node assigned
            if pod.spec.as_ref().and_then(|s| s.node_name.as_ref()).is_some() {
                continue;
            }

            // Skip pods being deleted
            if pod.metadata.deletion_timestamp.is_some() {
                continue;
            }

            // Try to schedule this pod
            match self.schedule(&pod).await {
                Ok(node_name) => {
                    // Update pod with assigned node
                    let mut pod = pod.clone();
                    if let Some(spec) = &mut pod.spec {
                        spec.node_name = Some(node_name.clone());
                    }

                    if let Err(e) = pod_store.update(pod.clone()).await {
                        error!("Failed to update pod {}: {}", pod.metadata.name, e);
                    } else {
                        info!(
                            "Scheduled pod {}/{} to node {}",
                            pod.metadata.effective_namespace(),
                            pod.metadata.name,
                            node_name
                        );
                    }
                }
                Err(SchedulerError::NoFeasibleNodes) => {
                    debug!(
                        "No feasible nodes for pod {}/{}",
                        pod.metadata.effective_namespace(),
                        pod.metadata.name
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to schedule pod {}/{}: {}",
                        pod.metadata.effective_namespace(),
                        pod.metadata.name,
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Schedule a pod to a node
    pub async fn schedule(&self, pod: &Pod) -> SchedulerResult<String> {
        // Get all nodes
        let node_store = ResourceStore::<Node>::new(self.storage.clone());
        let nodes = node_store.list(None).await?;

        if nodes.is_empty() {
            return Err(SchedulerError::NoNodesAvailable);
        }

        // Filter nodes
        let mut feasible_nodes = Vec::new();
        for node in &nodes {
            let mut passes = true;
            for filter in &self.filter_plugins {
                if !filter.filter(pod, node).await {
                    debug!(
                        "Node {} filtered by {} for pod {}",
                        node.metadata.name,
                        filter.name(),
                        pod.metadata.name
                    );
                    passes = false;
                    break;
                }
            }
            if passes {
                feasible_nodes.push(node);
            }
        }

        if feasible_nodes.is_empty() {
            return Err(SchedulerError::NoFeasibleNodes);
        }

        // Score nodes
        let mut scores: Vec<(&Node, i64)> = Vec::new();
        for node in &feasible_nodes {
            let mut total_score = 0i64;
            for plugin in &self.score_plugins {
                let score = plugin.score(pod, node).await;
                total_score += score;
                debug!(
                    "Node {} scored {} by {} for pod {}",
                    node.metadata.name,
                    score,
                    plugin.name(),
                    pod.metadata.name
                );
            }
            scores.push((node, total_score));
        }

        // Select highest scoring node
        scores.sort_by(|a, b| b.1.cmp(&a.1));
        let selected = scores.first().map(|(n, _)| n.metadata.name.clone());

        match selected {
            Some(name) => Ok(name),
            None => Err(SchedulerError::NoFeasibleNodes),
        }
    }
}

// Built-in filter plugins

struct NodeReadyFilter;

#[async_trait]
impl FilterPlugin for NodeReadyFilter {
    fn name(&self) -> &str {
        "NodeReady"
    }

    async fn filter(&self, _pod: &Pod, node: &Node) -> bool {
        if let Some(status) = &node.status {
            for condition in &status.conditions {
                if matches!(condition.r#type, k1s_types::NodeConditionType::Ready) {
                    return condition.status == "True";
                }
            }
        }
        false
    }
}

struct ResourceFitFilter;

#[async_trait]
impl FilterPlugin for ResourceFitFilter {
    fn name(&self) -> &str {
        "ResourceFit"
    }

    async fn filter(&self, _pod: &Pod, _node: &Node) -> bool {
        // TODO: Check if node has enough resources
        // For now, accept all nodes
        true
    }
}

struct NodeSelectorFilter;

#[async_trait]
impl FilterPlugin for NodeSelectorFilter {
    fn name(&self) -> &str {
        "NodeSelector"
    }

    async fn filter(&self, pod: &Pod, node: &Node) -> bool {
        if let Some(spec) = &pod.spec {
            if spec.node_selector.is_empty() {
                return true;
            }

            for (key, value) in &spec.node_selector {
                match node.metadata.labels.get(key) {
                    Some(node_value) if node_value == value => continue,
                    _ => return false,
                }
            }
        }
        true
    }
}

struct TaintTolerationFilter;

#[async_trait]
impl FilterPlugin for TaintTolerationFilter {
    fn name(&self) -> &str {
        "TaintToleration"
    }

    async fn filter(&self, pod: &Pod, node: &Node) -> bool {
        if let Some(spec) = &node.spec {
            for taint in &spec.taints {
                let tolerated = pod.spec.as_ref().map_or(false, |ps| {
                    ps.tolerations.iter().any(|t| {
                        t.key.as_deref() == Some(&taint.key)
                            && (t.operator.as_deref() == Some("Exists")
                                || t.value.as_deref() == taint.value.as_deref())
                    })
                });
                if !tolerated {
                    return false;
                }
            }
        }
        true
    }
}

// Built-in score plugins

struct LeastRequestedScore;

#[async_trait]
impl ScorePlugin for LeastRequestedScore {
    fn name(&self) -> &str {
        "LeastRequested"
    }

    async fn score(&self, _pod: &Pod, _node: &Node) -> i64 {
        // TODO: Calculate based on resource utilization
        // Higher score = more available resources
        50
    }
}

struct BalancedResourceScore;

#[async_trait]
impl ScorePlugin for BalancedResourceScore {
    fn name(&self) -> &str {
        "BalancedResource"
    }

    async fn score(&self, _pod: &Pod, _node: &Node) -> i64 {
        // TODO: Calculate based on resource balance
        // Higher score = more balanced resource usage
        50
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert_eq!(config.schedule_interval, Duration::from_secs(1));
    }
}
