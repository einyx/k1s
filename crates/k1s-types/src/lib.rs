//! Kubernetes resource types for k1s
//!
//! This crate provides the core Kubernetes resource types used throughout k1s,
//! including Pod, Namespace, Node, ConfigMap, Secret, Service, and workload controllers.

pub mod api;
pub mod meta;
pub mod resource;

pub use api::apps::v1 as apps_v1;
pub use api::apps::v1::{
    Deployment, DeploymentSpec, DeploymentStatus, DeploymentStrategy, DeploymentStrategyType,
    LabelSelector, PodTemplateSpec, ReplicaSet, ReplicaSetSpec, ReplicaSetStatus,
    RollingUpdateDeployment,
};
pub use api::core::v1::*;
pub use meta::{ListMeta, ObjectMeta, OwnerReference, TypeMeta};
pub use resource::{Resource, ResourceList, ResourceScope};
