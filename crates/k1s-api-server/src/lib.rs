//! Axum-based Kubernetes API server for k1s
//!
//! Provides REST API endpoints compatible with the Kubernetes API:
//! - CRUD operations for all core resources
//! - Watch endpoints for real-time updates
//! - Health and readiness probes
//! - Authentication and RBAC authorization

pub mod auth;
pub mod authz;
pub mod error;
pub mod handlers;
pub mod routes;
pub mod server;
pub mod state;
pub mod tls;
pub mod webhooks;

pub use auth::UserInfo;
pub use error::ApiError;
pub use server::ApiServer;
pub use state::AppState;
pub use tls::{TlsCerts, TlsConfig};
pub use webhooks::{WebhookState, admission_webhook_middleware};
