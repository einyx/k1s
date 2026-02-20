//! Kubernetes resource types for k1s
//!
//! This crate provides the core Kubernetes resource types used throughout k1s,
//! including Pod, Namespace, Node, ConfigMap, Secret, Service, and workload controllers.

pub mod api;
pub mod meta;
pub mod resource;
pub mod selector;

pub use api::apps::v1 as apps_v1;
pub use api::apps::v1::{
    DaemonSet, DaemonSetCondition, DaemonSetSpec, DaemonSetStatus, DaemonSetUpdateStrategy,
    Deployment, DeploymentSpec, DeploymentStatus, DeploymentStrategy, DeploymentStrategyType,
    LabelSelector, LabelSelectorRequirement, PodTemplateSpec, ReplicaSet, ReplicaSetSpec, ReplicaSetStatus,
    RollingUpdateDaemonSet, RollingUpdateDeployment, StatefulSet, StatefulSetSpec, StatefulSetStatus,
};
pub use api::batch::v1 as batch_v1;
pub use api::batch::v1::{
    condition_type, ConcurrencyPolicy, CronJob, CronJobSpec, CronJobStatus, Job, JobCondition,
    JobSpec, JobStatus, JobTemplateSpec,
};
pub use api::core::v1::*;
pub use api::rbac::v1 as rbac_v1;
pub use api::rbac::v1::{
    ClusterRole, ClusterRoleBinding, PolicyRule, Role, RoleBinding, RoleRef, Subject,
};
pub use api::storage::v1 as storage_v1;
pub use api::storage::v1::{StorageClass, VolumeBindingMode};
pub use meta::{ListMeta, ObjectMeta, OwnerReference, TypeMeta};
pub use resource::{Resource, ResourceList, ResourceScope};
// Re-export selector parsing utilities (not the K8s LabelSelector type)
pub use selector::LabelSelector as ParsedLabelSelector;
pub use selector::FieldSelector as ParsedFieldSelector;
