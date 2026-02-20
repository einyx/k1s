//! Health probe execution for containers
//!
//! Implements liveness, readiness, and startup probes using:
//! - HTTP GET requests
//! - TCP socket connections
//! - Exec commands inside containers

use std::sync::Arc;
use std::time::Duration;

use k1s_types::{ExecAction, HttpGetAction, IntOrString, Probe, TcpSocketAction};
use tracing::{debug, warn};

use crate::runtime::ContainerRuntime;
use crate::{KubeletError, KubeletResult};

/// Result of a probe execution
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProbeResult {
    /// Probe succeeded
    Success,
    /// Probe failed
    Failure,
    /// Probe result is unknown (e.g., timeout)
    Unknown,
}

/// Probe executor for running health checks
pub struct ProbeExecutor {
    runtime: Arc<dyn ContainerRuntime>,
}

impl ProbeExecutor {
    pub fn new(runtime: Arc<dyn ContainerRuntime>) -> Self {
        Self { runtime }
    }

    /// Execute a probe against a container
    pub async fn execute_probe(
        &self,
        probe: &Probe,
        container_id: &str,
        pod_ip: Option<&str>,
    ) -> ProbeResult {
        let timeout = if probe.timeout_seconds > 0 {
            Duration::from_secs(probe.timeout_seconds as u64)
        } else {
            Duration::from_secs(1)
        };

        // Try each probe type in order
        if let Some(http) = &probe.http_get {
            return self.execute_http_probe(http, pod_ip, timeout).await;
        }

        if let Some(tcp) = &probe.tcp_socket {
            return self.execute_tcp_probe(tcp, pod_ip, timeout).await;
        }

        if let Some(exec) = &probe.exec {
            return self.execute_exec_probe(exec, container_id, timeout).await;
        }

        // No probe action defined
        ProbeResult::Success
    }

    /// Execute an HTTP GET probe
    async fn execute_http_probe(
        &self,
        action: &HttpGetAction,
        pod_ip: Option<&str>,
        timeout: Duration,
    ) -> ProbeResult {
        let host = action.host.as_deref().or(pod_ip).unwrap_or("localhost");
        let port = match &action.port {
            IntOrString::Int(p) => *p,
            IntOrString::String(s) => s.parse().unwrap_or(80),
        };
        let path = action.path.as_deref().unwrap_or("/");
        let scheme = action.scheme.as_deref().unwrap_or("HTTP").to_lowercase();

        let url = format!("{}://{}:{}{}", scheme, host, port, path);
        debug!("Executing HTTP probe: {}", url);

        // Simple HTTP GET using TCP
        match tokio::time::timeout(timeout, async {
            self.simple_http_get(&host, port as u16, path, &scheme).await
        }).await {
            Ok(Ok(status)) => {
                if status >= 200 && status < 400 {
                    ProbeResult::Success
                } else {
                    warn!("HTTP probe failed with status {}", status);
                    ProbeResult::Failure
                }
            }
            Ok(Err(e)) => {
                warn!("HTTP probe error: {}", e);
                ProbeResult::Failure
            }
            Err(_) => {
                warn!("HTTP probe timed out");
                ProbeResult::Failure
            }
        }
    }

    /// Simple HTTP GET implementation
    async fn simple_http_get(
        &self,
        host: &str,
        port: u16,
        path: &str,
        _scheme: &str,
    ) -> KubeletResult<u16> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr).await.map_err(|e| {
            KubeletError::Runtime(format!("Failed to connect to {}: {}", addr, e))
        })?;

        // Send HTTP request
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host
        );
        stream.write_all(request.as_bytes()).await.map_err(|e| {
            KubeletError::Runtime(format!("Failed to send HTTP request: {}", e))
        })?;

        // Read response
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.map_err(|e| {
            KubeletError::Runtime(format!("Failed to read HTTP response: {}", e))
        })?;

        // Parse status code
        let response_str = String::from_utf8_lossy(&response);
        if let Some(status_line) = response_str.lines().next() {
            if let Some(code) = status_line.split_whitespace().nth(1) {
                if let Ok(status) = code.parse::<u16>() {
                    return Ok(status);
                }
            }
        }

        Err(KubeletError::Runtime("Invalid HTTP response".to_string()))
    }

    /// Execute a TCP socket probe
    async fn execute_tcp_probe(
        &self,
        action: &TcpSocketAction,
        pod_ip: Option<&str>,
        timeout: Duration,
    ) -> ProbeResult {
        let host = action.host.as_deref().or(pod_ip).unwrap_or("localhost");
        let port = match &action.port {
            IntOrString::Int(p) => *p as u16,
            IntOrString::String(s) => s.parse().unwrap_or(80),
        };

        let addr = format!("{}:{}", host, port);
        debug!("Executing TCP probe: {}", addr);

        match tokio::time::timeout(timeout, async {
            tokio::net::TcpStream::connect(&addr).await
        }).await {
            Ok(Ok(_)) => ProbeResult::Success,
            Ok(Err(e)) => {
                warn!("TCP probe failed: {}", e);
                ProbeResult::Failure
            }
            Err(_) => {
                warn!("TCP probe timed out");
                ProbeResult::Failure
            }
        }
    }

    /// Execute an exec probe (run command inside container)
    async fn execute_exec_probe(
        &self,
        action: &ExecAction,
        container_id: &str,
        timeout: Duration,
    ) -> ProbeResult {
        if action.command.is_empty() {
            return ProbeResult::Success;
        }

        debug!("Executing exec probe: {:?}", action.command);

        match tokio::time::timeout(timeout, async {
            self.runtime.exec(container_id, &action.command).await
        }).await {
            Ok(Ok((exit_code, _stdout, _stderr))) => {
                if exit_code == 0 {
                    ProbeResult::Success
                } else {
                    warn!("Exec probe failed with exit code {}", exit_code);
                    ProbeResult::Failure
                }
            }
            Ok(Err(e)) => {
                warn!("Exec probe error: {}", e);
                ProbeResult::Failure
            }
            Err(_) => {
                warn!("Exec probe timed out");
                ProbeResult::Failure
            }
        }
    }
}

/// Probe state tracker for a container
#[derive(Debug, Clone)]
pub struct ProbeState {
    /// Number of consecutive successes
    pub success_count: i32,
    /// Number of consecutive failures
    pub failure_count: i32,
    /// Whether the probe has ever succeeded (for startup probes)
    pub has_succeeded: bool,
}

impl Default for ProbeState {
    fn default() -> Self {
        Self {
            success_count: 0,
            failure_count: 0,
            has_succeeded: false,
        }
    }
}

impl ProbeState {
    /// Update state based on probe result
    pub fn record_result(&mut self, result: ProbeResult) {
        match result {
            ProbeResult::Success => {
                self.success_count += 1;
                self.failure_count = 0;
                self.has_succeeded = true;
            }
            ProbeResult::Failure => {
                self.failure_count += 1;
                self.success_count = 0;
            }
            ProbeResult::Unknown => {
                // Unknown counts as failure
                self.failure_count += 1;
                self.success_count = 0;
            }
        }
    }

    /// Check if probe is considered successful (meets success threshold)
    pub fn is_successful(&self, threshold: i32) -> bool {
        let threshold = if threshold > 0 { threshold } else { 1 };
        self.success_count >= threshold
    }

    /// Check if probe is considered failed (meets failure threshold)
    pub fn is_failed(&self, threshold: i32) -> bool {
        let threshold = if threshold > 0 { threshold } else { 3 };
        self.failure_count >= threshold
    }

    /// Reset the probe state after container restart
    pub fn reset(&mut self) {
        self.success_count = 0;
        self.failure_count = 0;
        // Keep has_succeeded true since we're restarting, not starting fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_state() {
        let mut state = ProbeState::default();

        // Initial state
        assert!(!state.is_successful(1));
        assert!(!state.is_failed(3));

        // Record success
        state.record_result(ProbeResult::Success);
        assert!(state.is_successful(1));
        assert!(!state.is_failed(3));

        // Record failures
        state.record_result(ProbeResult::Failure);
        state.record_result(ProbeResult::Failure);
        state.record_result(ProbeResult::Failure);
        assert!(!state.is_successful(1));
        assert!(state.is_failed(3));
    }
}
