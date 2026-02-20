//! API request handlers

pub mod apps;
pub mod batch;
pub mod core;
pub mod events;
pub mod generic;
pub mod health;
pub mod list_watch;
pub mod openapi;
pub mod patch;
pub mod pod_subresources;
pub mod rbac;
pub mod scale;
pub mod storage;
pub mod watch;

pub use health::{healthz, livez, readyz};
