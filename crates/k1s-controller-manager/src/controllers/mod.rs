//! Built-in controllers

mod cronjob;
mod daemonset;
mod deployment;
mod endpoints;
mod job;
mod namespace;
mod pv_pvc;
mod replicaset;
mod statefulset;

pub use cronjob::CronJobController;
pub use daemonset::DaemonSetController;
pub use deployment::DeploymentController;
pub use endpoints::EndpointsController;
pub use job::JobController;
pub use namespace::NamespaceController;
pub use pv_pvc::PvPvcController;
pub use replicaset::ReplicaSetController;
pub use statefulset::StatefulSetController;
