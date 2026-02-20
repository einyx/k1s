//! Pod scheduler with filter/score plugins
//!
//! Watches for unscheduled pods and assigns them to suitable nodes
//! based on resource requirements, constraints, and scoring.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use k1s_storage::backend::ResourceStore;
use k1s_storage::SledBackend;
use k1s_types::{Node, Pod};
use thiserror::Error;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Parse Kubernetes resource quantity string to milliunits
/// CPU: "100m" -> 100, "1" -> 1000, "0.5" -> 500
/// Memory: "128Mi" -> 128*1024*1024, "1Gi" -> 1024*1024*1024
fn parse_resource_quantity(value: &str) -> i64 {
    let value = value.trim();

    // Handle CPU millicores (e.g., "100m")
    if value.ends_with('m') {
        return value[..value.len()-1].parse::<i64>().unwrap_or(0);
    }

    // Handle memory with binary suffixes
    if value.ends_with("Ki") {
        return value[..value.len()-2].parse::<i64>().unwrap_or(0) * 1024;
    }
    if value.ends_with("Mi") {
        return value[..value.len()-2].parse::<i64>().unwrap_or(0) * 1024 * 1024;
    }
    if value.ends_with("Gi") {
        return value[..value.len()-2].parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024;
    }
    if value.ends_with("Ti") {
        return value[..value.len()-2].parse::<i64>().unwrap_or(0) * 1024 * 1024 * 1024 * 1024;
    }

    // Handle memory with decimal suffixes
    if value.ends_with('K') || value.ends_with('k') {
        return value[..value.len()-1].parse::<i64>().unwrap_or(0) * 1000;
    }
    if value.ends_with('M') {
        return value[..value.len()-1].parse::<i64>().unwrap_or(0) * 1000 * 1000;
    }
    if value.ends_with('G') {
        return value[..value.len()-1].parse::<i64>().unwrap_or(0) * 1000 * 1000 * 1000;
    }
    if value.ends_with('T') {
        return value[..value.len()-1].parse::<i64>().unwrap_or(0) * 1000 * 1000 * 1000 * 1000;
    }

    // Plain number - for CPU, convert to millicores
    if let Ok(n) = value.parse::<f64>() {
        return (n * 1000.0) as i64;
    }

    value.parse::<i64>().unwrap_or(0)
}

/// Get total resource requests from a pod
fn get_pod_requests(pod: &Pod) -> BTreeMap<String, i64> {
    let mut requests = BTreeMap::new();

    if let Some(spec) = &pod.spec {
        for container in &spec.containers {
            if let Some(res) = &container.resources {
                for (key, value) in &res.requests {
                    let parsed = parse_resource_quantity(value);
                    *requests.entry(key.clone()).or_insert(0) += parsed;
                }
            }
        }
        for container in &spec.init_containers {
            if let Some(res) = &container.resources {
                for (key, value) in &res.requests {
                    let parsed = parse_resource_quantity(value);
                    // Init containers run sequentially, take max not sum
                    let entry = requests.entry(key.clone()).or_insert(0);
                    *entry = (*entry).max(parsed);
                }
            }
        }
    }

    requests
}

/// Get allocatable resources from a node
fn get_node_allocatable(node: &Node) -> BTreeMap<String, i64> {
    let mut allocatable = BTreeMap::new();

    if let Some(status) = &node.status {
        for (key, value) in &status.allocatable {
            allocatable.insert(key.clone(), parse_resource_quantity(value));
        }
    }

    allocatable
}

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
                Box::new(NodeAffinityFilter),
            ],
            score_plugins: vec![
                Box::new(LeastRequestedScore),
                Box::new(BalancedResourceScore),
                Box::new(NodeAffinityScore),
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

    async fn filter(&self, pod: &Pod, node: &Node) -> bool {
        let pod_requests = get_pod_requests(pod);
        let node_allocatable = get_node_allocatable(node);

        // Check CPU
        if let Some(&requested_cpu) = pod_requests.get("cpu") {
            let available_cpu = node_allocatable.get("cpu").copied().unwrap_or(0);
            if requested_cpu > available_cpu {
                debug!(
                    "Node {} has insufficient CPU: requested={}, available={}",
                    node.metadata.name, requested_cpu, available_cpu
                );
                return false;
            }
        }

        // Check memory
        if let Some(&requested_mem) = pod_requests.get("memory") {
            let available_mem = node_allocatable.get("memory").copied().unwrap_or(0);
            if requested_mem > available_mem {
                debug!(
                    "Node {} has insufficient memory: requested={}, available={}",
                    node.metadata.name, requested_mem, available_mem
                );
                return false;
            }
        }

        // Check ephemeral-storage
        if let Some(&requested_storage) = pod_requests.get("ephemeral-storage") {
            let available_storage = node_allocatable.get("ephemeral-storage").copied().unwrap_or(0);
            if requested_storage > available_storage {
                debug!(
                    "Node {} has insufficient ephemeral storage: requested={}, available={}",
                    node.metadata.name, requested_storage, available_storage
                );
                return false;
            }
        }

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

struct NodeAffinityFilter;

#[async_trait]
impl FilterPlugin for NodeAffinityFilter {
    fn name(&self) -> &str {
        "NodeAffinity"
    }

    async fn filter(&self, pod: &Pod, node: &Node) -> bool {
        let affinity = match pod.spec.as_ref().and_then(|s| s.affinity.as_ref()) {
            Some(a) => a,
            None => return true, // No affinity rules
        };

        let node_affinity = match &affinity.node_affinity {
            Some(na) => na,
            None => return true, // No node affinity rules
        };

        // Check requiredDuringSchedulingIgnoredDuringExecution
        if let Some(required) = &node_affinity.required_during_scheduling_ignored_during_execution {
            // Must match at least one NodeSelectorTerm
            let matches_any_term = required.node_selector_terms.iter().any(|term| {
                // All match_expressions must match
                let expressions_match = term.match_expressions.iter().all(|expr| {
                    match_selector_requirement(&expr.key, &expr.operator, &expr.values, &node.metadata.labels)
                });

                // All match_fields must match
                let fields_match = term.match_fields.iter().all(|expr| {
                    match_field_requirement(&expr.key, &expr.operator, &expr.values, node)
                });

                expressions_match && fields_match
            });

            if !matches_any_term && !required.node_selector_terms.is_empty() {
                debug!(
                    "Node {} does not match required node affinity for pod {}",
                    node.metadata.name, pod.metadata.name
                );
                return false;
            }
        }

        true
    }
}

/// Check if a node label matches a selector requirement
fn match_selector_requirement(
    key: &str,
    operator: &str,
    values: &[String],
    labels: &BTreeMap<String, String>,
) -> bool {
    let value = labels.get(key);

    match operator {
        "In" => value.map_or(false, |v| values.contains(v)),
        "NotIn" => value.map_or(true, |v| !values.contains(v)),
        "Exists" => value.is_some(),
        "DoesNotExist" => value.is_none(),
        "Gt" => {
            if let (Some(node_val), Some(req_val)) = (
                value.and_then(|v| v.parse::<i64>().ok()),
                values.first().and_then(|v| v.parse::<i64>().ok()),
            ) {
                node_val > req_val
            } else {
                false
            }
        }
        "Lt" => {
            if let (Some(node_val), Some(req_val)) = (
                value.and_then(|v| v.parse::<i64>().ok()),
                values.first().and_then(|v| v.parse::<i64>().ok()),
            ) {
                node_val < req_val
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a node field matches a selector requirement
fn match_field_requirement(
    key: &str,
    operator: &str,
    values: &[String],
    node: &Node,
) -> bool {
    let value = match key {
        "metadata.name" => Some(node.metadata.name.clone()),
        "spec.unschedulable" => Some(
            node.spec
                .as_ref()
                .map_or(false, |s| s.unschedulable)
                .to_string(),
        ),
        _ => None,
    };

    match operator {
        "In" => value.map_or(false, |v| values.contains(&v)),
        "NotIn" => value.map_or(true, |v| !values.contains(&v)),
        _ => false,
    }
}

// Built-in score plugins

struct LeastRequestedScore;

#[async_trait]
impl ScorePlugin for LeastRequestedScore {
    fn name(&self) -> &str {
        "LeastRequested"
    }

    async fn score(&self, pod: &Pod, node: &Node) -> i64 {
        let pod_requests = get_pod_requests(pod);
        let node_allocatable = get_node_allocatable(node);

        // Calculate available resources after scheduling this pod
        // Score: (allocatable - requested) / allocatable * 100
        let mut total_score = 0i64;
        let mut num_resources = 0;

        // Score CPU (weight: 1)
        if let Some(&allocatable) = node_allocatable.get("cpu") {
            if allocatable > 0 {
                let requested = pod_requests.get("cpu").copied().unwrap_or(0);
                let remaining = allocatable.saturating_sub(requested);
                let score = (remaining * 100) / allocatable;
                total_score += score;
                num_resources += 1;
            }
        }

        // Score memory (weight: 1)
        if let Some(&allocatable) = node_allocatable.get("memory") {
            if allocatable > 0 {
                let requested = pod_requests.get("memory").copied().unwrap_or(0);
                let remaining = allocatable.saturating_sub(requested);
                let score = (remaining * 100) / allocatable;
                total_score += score;
                num_resources += 1;
            }
        }

        if num_resources > 0 {
            total_score / num_resources
        } else {
            50 // Default score if no resource info available
        }
    }
}

struct BalancedResourceScore;

#[async_trait]
impl ScorePlugin for BalancedResourceScore {
    fn name(&self) -> &str {
        "BalancedResource"
    }

    async fn score(&self, pod: &Pod, node: &Node) -> i64 {
        let pod_requests = get_pod_requests(pod);
        let node_allocatable = get_node_allocatable(node);

        // Calculate resource usage fractions and score based on balance
        // More balanced usage = higher score
        let mut cpu_fraction = 0.5f64;
        let mut mem_fraction = 0.5f64;

        if let Some(&allocatable) = node_allocatable.get("cpu") {
            if allocatable > 0 {
                let requested = pod_requests.get("cpu").copied().unwrap_or(0);
                cpu_fraction = requested as f64 / allocatable as f64;
            }
        }

        if let Some(&allocatable) = node_allocatable.get("memory") {
            if allocatable > 0 {
                let requested = pod_requests.get("memory").copied().unwrap_or(0);
                mem_fraction = requested as f64 / allocatable as f64;
            }
        }

        // Score is higher when CPU and memory usage are balanced
        // Max score (100) when fractions are equal, lower when imbalanced
        let diff = (cpu_fraction - mem_fraction).abs();
        let score = ((1.0 - diff) * 100.0) as i64;
        score.clamp(0, 100)
    }
}

struct NodeAffinityScore;

#[async_trait]
impl ScorePlugin for NodeAffinityScore {
    fn name(&self) -> &str {
        "NodeAffinity"
    }

    async fn score(&self, pod: &Pod, node: &Node) -> i64 {
        let affinity = match pod.spec.as_ref().and_then(|s| s.affinity.as_ref()) {
            Some(a) => a,
            None => return 0,
        };

        let node_affinity = match &affinity.node_affinity {
            Some(na) => na,
            None => return 0,
        };

        // Score based on preferredDuringSchedulingIgnoredDuringExecution
        let mut total_weight = 0i32;

        for pref in &node_affinity.preferred_during_scheduling_ignored_during_execution {
            let term = &pref.preference;

            // Check if this node matches the preference
            let expressions_match = term.match_expressions.iter().all(|expr| {
                match_selector_requirement(&expr.key, &expr.operator, &expr.values, &node.metadata.labels)
            });

            let fields_match = term.match_fields.iter().all(|expr| {
                match_field_requirement(&expr.key, &expr.operator, &expr.values, node)
            });

            if expressions_match && fields_match {
                total_weight += pref.weight;
            }
        }

        // Normalize to 0-100 scale (weights typically range 1-100)
        (total_weight as i64).clamp(0, 100)
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

    #[test]
    fn test_parse_cpu() {
        assert_eq!(parse_resource_quantity("100m"), 100);
        assert_eq!(parse_resource_quantity("1"), 1000);
        assert_eq!(parse_resource_quantity("0.5"), 500);
        assert_eq!(parse_resource_quantity("2"), 2000);
        assert_eq!(parse_resource_quantity("250m"), 250);
    }

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_resource_quantity("128Mi"), 128 * 1024 * 1024);
        assert_eq!(parse_resource_quantity("1Gi"), 1024 * 1024 * 1024);
        assert_eq!(parse_resource_quantity("512Ki"), 512 * 1024);
        assert_eq!(parse_resource_quantity("100M"), 100 * 1000 * 1000);
        assert_eq!(parse_resource_quantity("1G"), 1000 * 1000 * 1000);
    }
}
