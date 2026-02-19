//! kube-proxy implementation

mod iptables;

pub use iptables::IptablesProxy;

use async_trait::async_trait;
use k1s_types::Service;

use crate::NetworkResult;

/// Service proxy trait
#[async_trait]
pub trait ServiceProxy: Send + Sync {
    /// Sync service rules
    async fn sync_service(&self, service: &Service) -> NetworkResult<()>;

    /// Remove service rules
    async fn remove_service(&self, service: &Service) -> NetworkResult<()>;

    /// Sync all services
    async fn sync_all(&self) -> NetworkResult<()>;
}
