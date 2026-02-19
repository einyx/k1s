//! CNI plugin support

mod executor;

pub use executor::CniExecutor;

use serde::{Deserialize, Serialize};

/// CNI network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CniConfig {
    pub cni_version: String,
    pub name: String,
    #[serde(rename = "type")]
    pub plugin_type: String,
    #[serde(flatten)]
    pub plugin_config: serde_json::Value,
}

/// CNI result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CniResult {
    #[serde(default)]
    pub ips: Vec<CniIp>,
    #[serde(default)]
    pub routes: Vec<CniRoute>,
    #[serde(default)]
    pub dns: CniDns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CniIp {
    pub address: String,
    #[serde(default)]
    pub gateway: Option<String>,
    #[serde(default)]
    pub interface: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CniRoute {
    pub dst: String,
    #[serde(default)]
    pub gw: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CniDns {
    #[serde(default)]
    pub nameservers: Vec<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub search: Vec<String>,
    #[serde(default)]
    pub options: Vec<String>,
}
