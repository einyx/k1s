//! Axum-based Kubernetes API server for k1s
//!
//! Provides REST API endpoints compatible with the Kubernetes API:
//! - CRUD operations for all core resources
//! - Watch endpoints for real-time updates
//! - Health and readiness probes

pub mod error;
pub mod handlers;
pub mod routes;
pub mod server;
pub mod state;
pub mod tls;

pub use error::ApiError;
pub use server::ApiServer;
pub use state::AppState;
pub use tls::{TlsCerts, TlsConfig};
