//! Docker Compose support
//!
//! This module provides:
//! - Parsing of Docker Compose files
//! - Translation to Kubernetes resources
//! - Native execution of compose services via Docker

mod executor;
mod parser;
mod translator;

pub use executor::ComposeExecutor;
pub use parser::ComposeFile;
pub use translator::ComposeTranslator;
