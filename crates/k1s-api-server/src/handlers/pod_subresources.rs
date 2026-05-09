//! Pod subresource handlers (logs, exec, attach, port-forward)

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::{Deserialize, Serialize};

use k1s_storage::ResourceStore;
use k1s_types::Pod;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for pod logs
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsQuery {
    /// Container name (optional, defaults to first container)
    #[serde(default)]
    pub container: Option<String>,
    /// Follow the log stream (not implemented yet)
    #[serde(default)]
    pub follow: Option<bool>,
    /// Number of lines from the end to show
    #[serde(default)]
    pub tail_lines: Option<u32>,
    /// Show timestamps
    #[serde(default)]
    pub timestamps: Option<bool>,
    /// Show previous container logs
    #[serde(default)]
    pub previous: Option<bool>,
    /// Only return logs after a specific date
    #[serde(default)]
    pub since_time: Option<String>,
    /// Only return logs newer than a relative duration
    #[serde(default)]
    pub since_seconds: Option<i64>,
}

/// Query parameters for pod exec
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecQuery {
    /// Container name (optional, defaults to first container)
    #[serde(default)]
    pub container: Option<String>,
    /// Command to execute (can be specified multiple times)
    #[serde(default)]
    pub command: Vec<String>,
    /// Pass stdin to the container
    #[serde(default)]
    pub stdin: Option<bool>,
    /// Stdout output
    #[serde(default)]
    pub stdout: Option<bool>,
    /// Stderr output
    #[serde(default)]
    pub stderr: Option<bool>,
    /// TTY allocation
    #[serde(default)]
    pub tty: Option<bool>,
}

/// Response for exec operations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Get container ID from pod status
fn get_container_id(pod: &Pod, container_name: Option<&str>) -> Option<String> {
    let status = pod.status.as_ref()?;

    // Determine which container to use
    let target_name = container_name.or_else(|| {
        pod.spec
            .as_ref()
            .and_then(|s| s.containers.first())
            .map(|c| c.name.as_str())
    })?;

    // Find container status with matching name
    for cs in &status.container_statuses {
        if cs.name == target_name {
            return cs.container_id.clone();
        }
    }

    None
}

/// Extract Docker container ID from k8s container ID format
/// Format: docker://<container_id> or containerd://<container_id>
fn extract_container_id(container_id: &str) -> &str {
    if let Some(id) = container_id.strip_prefix("docker://") {
        return id;
    }
    if let Some(id) = container_id.strip_prefix("containerd://") {
        return id;
    }
    container_id
}

/// Get logs from a pod container
///
/// GET /api/v1/namespaces/{namespace}/pods/{name}/log
pub async fn pod_logs(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(params): Query<LogsQuery>,
) -> ApiResult<Response> {
    let pod_store = ResourceStore::<Pod>::new(state.storage.clone());

    // Get the pod
    let pod = pod_store
        .get(Some(&namespace), &name)
        .await?
        .ok_or_else(|| ApiError::NotFound {
            kind: "Pod".to_string(),
            name: name.clone(),
        })?;

    // Get container ID from pod status
    let container_id = get_container_id(&pod, params.container.as_deref()).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "No container found in pod {name} (container status may not be available yet)"
        ))
    })?;

    let container_id = extract_container_id(&container_id);

    // Connect to Docker runtime directly
    // In a real implementation, this would proxy through the kubelet
    let runtime = match create_docker_runtime().await {
        Ok(rt) => rt,
        Err(e) => {
            return Err(ApiError::Internal(format!(
                "Failed to connect to container runtime: {e}"
            )));
        }
    };

    // Get logs
    let logs = runtime
        .logs(container_id, params.tail_lines)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to get logs: {e}")))?;

    // Return as plain text
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(logs))
        .unwrap())
}

/// Execute a command in a pod container
///
/// POST /api/v1/namespaces/{namespace}/pods/{name}/exec
pub async fn pod_exec(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
    Query(params): Query<ExecQuery>,
) -> ApiResult<Json<ExecResponse>> {
    let pod_store = ResourceStore::<Pod>::new(state.storage.clone());

    // Get the pod
    let pod = pod_store
        .get(Some(&namespace), &name)
        .await?
        .ok_or_else(|| ApiError::NotFound {
            kind: "Pod".to_string(),
            name: name.clone(),
        })?;

    // Get container ID from pod status
    let container_id = get_container_id(&pod, params.container.as_deref()).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "No container found in pod {name} (container status may not be available yet)"
        ))
    })?;

    let container_id = extract_container_id(&container_id);

    if params.command.is_empty() {
        return Err(ApiError::BadRequest("No command specified".to_string()));
    }

    // Connect to Docker runtime directly
    let runtime = match create_docker_runtime().await {
        Ok(rt) => rt,
        Err(e) => {
            return Err(ApiError::Internal(format!(
                "Failed to connect to container runtime: {e}"
            )));
        }
    };

    // Execute command
    let (exit_code, stdout, stderr) = runtime
        .exec(container_id, &params.command)
        .await
        .map_err(|e| ApiError::Internal(format!("Failed to exec: {e}")))?;

    Ok(Json(ExecResponse {
        exit_code,
        stdout,
        stderr,
    }))
}

// Helper to create Docker runtime
// This is a simplified version - in production, would use shared runtime
async fn create_docker_runtime() -> Result<DockerRuntimeWrapper, String> {
    DockerRuntimeWrapper::new().await
}

/// Wrapper around bollard Docker client for API server use
struct DockerRuntimeWrapper {
    client: bollard::Docker,
}

impl DockerRuntimeWrapper {
    async fn new() -> Result<Self, String> {
        // Try common Docker socket locations
        let socket_paths = [
            "/var/run/docker.sock",
            &format!(
                "{}/.docker/run/docker.sock",
                std::env::var("HOME").unwrap_or_default()
            ),
            &format!(
                "{}/.colima/default/docker.sock",
                std::env::var("HOME").unwrap_or_default()
            ),
        ];

        for path in &socket_paths {
            if std::path::Path::new(path).exists() {
                if let Ok(client) =
                    bollard::Docker::connect_with_socket(path, 120, bollard::API_DEFAULT_VERSION)
                {
                    if client.ping().await.is_ok() {
                        return Ok(Self { client });
                    }
                }
            }
        }

        // Fall back to bollard's default detection
        bollard::Docker::connect_with_local_defaults()
            .map(|client| Self { client })
            .map_err(|e| e.to_string())
    }

    async fn logs(&self, container_id: &str, tail: Option<u32>) -> Result<String, String> {
        use bollard::container::LogsOptions;
        use futures::StreamExt;

        let options = Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            tail: tail
                .map(|t| t.to_string())
                .unwrap_or_else(|| "100".to_string()),
            ..Default::default()
        });

        let mut stream = self.client.logs(container_id, options);
        let mut output = String::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(log) => output.push_str(&log.to_string()),
                Err(e) => return Err(e.to_string()),
            }
        }

        Ok(output)
    }

    async fn exec(
        &self,
        container_id: &str,
        command: &[String],
    ) -> Result<(i32, String, String), String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures::StreamExt;

        let exec = self
            .client
            .create_exec(
                container_id,
                CreateExecOptions {
                    cmd: Some(command.to_vec()),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        let output = self
            .client
            .start_exec(&exec.id, None)
            .await
            .map_err(|e| e.to_string())?;

        let mut stdout = String::new();
        let mut stderr = String::new();

        if let StartExecResults::Attached { mut output, .. } = output {
            while let Some(result) = output.next().await {
                match result {
                    Ok(log) => match log {
                        bollard::container::LogOutput::StdOut { message } => {
                            stdout.push_str(&String::from_utf8_lossy(&message));
                        }
                        bollard::container::LogOutput::StdErr { message } => {
                            stderr.push_str(&String::from_utf8_lossy(&message));
                        }
                        _ => {}
                    },
                    Err(e) => return Err(e.to_string()),
                }
            }
        }

        // Get exit code
        let inspect = self
            .client
            .inspect_exec(&exec.id)
            .await
            .map_err(|e| e.to_string())?;

        let exit_code = inspect.exit_code.unwrap_or(0) as i32;

        Ok((exit_code, stdout, stderr))
    }
}
