//! CNI and kube-proxy networking for k1s

pub mod cni;
pub mod proxy;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("CNI error: {0}")]
    Cni(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type NetworkResult<T> = Result<T, NetworkError>;
