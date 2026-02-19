//! Built-in controllers

mod deployment;
mod endpoints;
mod namespace;
mod replicaset;

pub use deployment::DeploymentController;
pub use endpoints::EndpointsController;
pub use namespace::NamespaceController;
pub use replicaset::ReplicaSetController;
