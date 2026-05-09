//! CNI plugin executor

use std::path::{Path, PathBuf};
use std::process::Command;

use super::{CniConfig, CniResult};
use crate::{NetworkError, NetworkResult};

/// CNI plugin executor
pub struct CniExecutor {
    bin_dir: PathBuf,
    conf_dir: PathBuf,
}

impl CniExecutor {
    pub fn new(bin_dir: impl AsRef<Path>, conf_dir: impl AsRef<Path>) -> Self {
        Self {
            bin_dir: bin_dir.as_ref().to_path_buf(),
            conf_dir: conf_dir.as_ref().to_path_buf(),
        }
    }

    /// Execute CNI ADD command
    pub fn add(
        &self,
        config: &CniConfig,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> NetworkResult<CniResult> {
        self.exec_plugin(config, "ADD", container_id, netns, ifname)
    }

    /// Execute CNI DEL command
    pub fn del(
        &self,
        config: &CniConfig,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> NetworkResult<()> {
        self.exec_plugin(config, "DEL", container_id, netns, ifname)?;
        Ok(())
    }

    /// Execute CNI CHECK command
    pub fn check(
        &self,
        config: &CniConfig,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> NetworkResult<()> {
        self.exec_plugin(config, "CHECK", container_id, netns, ifname)?;
        Ok(())
    }

    fn exec_plugin(
        &self,
        config: &CniConfig,
        command: &str,
        container_id: &str,
        netns: &str,
        ifname: &str,
    ) -> NetworkResult<CniResult> {
        let plugin_path = self.bin_dir.join(&config.plugin_type);

        if !plugin_path.exists() {
            return Err(NetworkError::Cni(format!(
                "CNI plugin not found: {}",
                config.plugin_type
            )));
        }

        let config_json = serde_json::to_string(config)
            .map_err(|e| NetworkError::Cni(e.to_string()))?;

        let output = Command::new(&plugin_path)
            .env("CNI_COMMAND", command)
            .env("CNI_CONTAINERID", container_id)
            .env("CNI_NETNS", netns)
            .env("CNI_IFNAME", ifname)
            .env("CNI_PATH", &self.bin_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| NetworkError::Cni(e.to_string()))?
            .wait_with_output()
            .map_err(|e| NetworkError::Cni(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NetworkError::Cni(format!(
                "CNI plugin failed: {stderr}"
            )));
        }

        if command == "ADD" {
            let result: CniResult = serde_json::from_slice(&output.stdout)
                .map_err(|e| NetworkError::Cni(e.to_string()))?;
            Ok(result)
        } else {
            Ok(CniResult {
                ips: vec![],
                routes: vec![],
                dns: Default::default(),
            })
        }
    }
}
