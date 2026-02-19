//! Common utilities for k1s

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub mod error;
pub mod id;

pub use error::{Error, Result};

/// Generate a unique resource UID
pub fn generate_uid() -> String {
    Uuid::new_v4().to_string()
}

/// Get current timestamp in RFC3339 format
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Generate a resource version (monotonically increasing)
pub fn generate_resource_version() -> String {
    // Use timestamp in microseconds for simple resource versioning
    Utc::now().timestamp_micros().to_string()
}
