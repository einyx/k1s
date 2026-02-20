//! Reconciliation controllers for k1s

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use k1s_storage::SledBackend;
use thiserror::Error;
use tokio::time::interval;
use tracing::{error, info};

pub mod controllers;

#[derive(Error, Debug)]
pub enum ControllerError {
    #[error("Storage error: {0}")]
    Storage(#[from] k1s_storage::StorageError),

    #[error("Reconciliation error: {0}")]
    Reconcile(String),
}

pub type ControllerResult<T> = Result<T, ControllerError>;

/// Controller trait for reconciliation loops
#[async_trait]
pub trait Controller: Send + Sync {
    fn name(&self) -> &str;
    async fn reconcile(&self) -> ControllerResult<()>;
}

/// Controller manager that runs all controllers
pub struct ControllerManager {
    storage: Arc<SledBackend>,
    controllers: Vec<Box<dyn Controller>>,
    reconcile_interval: Duration,
}

impl ControllerManager {
    pub fn new(storage: Arc<SledBackend>) -> Self {
        Self {
            storage: storage.clone(),
            controllers: vec![
                Box::new(controllers::NamespaceController::new(storage.clone())),
                Box::new(controllers::ReplicaSetController::new(storage.clone())),
                Box::new(controllers::DeploymentController::new(storage.clone())),
                Box::new(controllers::EndpointsController::new(storage.clone())),
                Box::new(controllers::JobController::new(storage.clone())),
                Box::new(controllers::CronJobController::new(storage.clone())),
                Box::new(controllers::DaemonSetController::new(storage.clone())),
                Box::new(controllers::StatefulSetController::new(storage.clone())),
                Box::new(controllers::PvPvcController::new(storage.clone())),
            ],
            reconcile_interval: Duration::from_secs(10),
        }
    }

    pub fn with_reconcile_interval(mut self, interval: Duration) -> Self {
        self.reconcile_interval = interval;
        self
    }

    /// Run all controllers
    pub async fn run(&self) {
        info!("Starting controller manager with {} controllers", self.controllers.len());

        let mut ticker = interval(self.reconcile_interval);

        loop {
            ticker.tick().await;

            for controller in &self.controllers {
                if let Err(e) = controller.reconcile().await {
                    error!("Controller {} failed: {}", controller.name(), e);
                }
            }
        }
    }
}
