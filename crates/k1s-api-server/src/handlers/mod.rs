//! API request handlers

pub mod apps;
pub mod core;
pub mod generic;
pub mod health;
pub mod pod_subresources;
pub mod watch;

pub use health::{healthz, livez, readyz};
