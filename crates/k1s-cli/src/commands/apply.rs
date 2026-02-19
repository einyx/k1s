//! Unified apply command with auto-detection of file formats
//!
//! Supports both:
//! - Docker Compose files -> Native container execution
//! - Kubernetes manifests -> API server submission

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use serde_json::Value;
use tracing::info;

use crate::compose::{ComposeExecutor, ComposeFile, ComposeTranslator};

/// Detected file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Compose,
    Kubernetes,
}

impl std::fmt::Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileType::Compose => write!(f, "Docker Compose"),
            FileType::Kubernetes => write!(f, "Kubernetes manifest"),
        }
    }
}

/// Apply configuration from file
#[derive(Args, Debug)]
pub struct ApplyArgs {
    /// File to apply
    #[arg(short, long)]
    pub filename: PathBuf,

    /// Namespace (for Kubernetes resources)
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Force Compose mode (translate and apply to K8s API)
    #[arg(long)]
    pub convert: bool,

    /// Detach after starting (compose mode)
    #[arg(short, long, default_value = "true")]
    pub detach: bool,

    /// Project name (compose mode, defaults to directory name)
    #[arg(long)]
    pub project_name: Option<String>,
}

/// Arguments for `up` command (compose-style)
#[derive(Args, Debug)]
pub struct UpArgs {
    /// Compose file
    #[arg(short, long, default_value = "docker-compose.yml")]
    pub file: PathBuf,

    /// Detach mode
    #[arg(short, long, default_value = "true")]
    pub detach: bool,

    /// Project name
    #[arg(short, long)]
    pub project_name: Option<String>,
}

/// Arguments for `down` command
#[derive(Args, Debug)]
pub struct DownArgs {
    /// Compose file
    #[arg(short, long, default_value = "docker-compose.yml")]
    pub file: PathBuf,

    /// Project name
    #[arg(short, long)]
    pub project_name: Option<String>,

    /// Remove named volumes
    #[arg(long)]
    pub volumes: bool,
}

/// Arguments for `ps` command
#[derive(Args, Debug)]
pub struct PsArgs {
    /// Filter by project name
    #[arg(short, long)]
    pub project_name: Option<String>,

    /// Show all containers (including stopped)
    #[arg(short, long)]
    pub all: bool,
}

/// Detect if content is Docker Compose or Kubernetes manifest
pub fn detect_file_type(content: &str) -> FileType {
    let trimmed = content.trim_start();

    // Quick heuristics: Compose files have "services:" at top level
    // K8s manifests have "apiVersion:" and "kind:"
    if let Ok(yaml) = serde_yaml::from_str::<Value>(trimmed) {
        if yaml.get("services").is_some() {
            return FileType::Compose;
        }

        if yaml.get("apiVersion").is_some() && yaml.get("kind").is_some() {
            return FileType::Kubernetes;
        }
    }

    // Check multi-document YAML
    for doc in content.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        if let Ok(yaml) = serde_yaml::from_str::<Value>(doc) {
            if yaml.get("apiVersion").is_some() && yaml.get("kind").is_some() {
                return FileType::Kubernetes;
            }
        }
    }

    // Default based on filename would be handled at call site
    FileType::Compose
}

/// Execute the unified apply command
pub async fn run_apply(args: ApplyArgs) -> Result<()> {
    let content = std::fs::read_to_string(&args.filename)
        .with_context(|| format!("Failed to read {}", args.filename.display()))?;

    let file_type = detect_file_type(&content);
    info!("Detected file type: {}", file_type);

    match (file_type, args.convert) {
        (FileType::Compose, false) => {
            run_compose_native(&args.filename, &content, args.project_name, args.detach).await
        }
        (FileType::Compose, true) => {
            run_compose_convert(&args.filename, &content, &args.namespace).await
        }
        (FileType::Kubernetes, _) => {
            run_kubernetes_apply(&content, &args.namespace).await
        }
    }
}

/// Run compose file natively using Docker
async fn run_compose_native(
    file_path: &Path,
    content: &str,
    project_name: Option<String>,
    detach: bool,
) -> Result<()> {
    let compose = ComposeFile::parse(content)
        .context("Failed to parse compose file")?;

    let project = project_name.unwrap_or_else(|| derive_project_name(file_path));

    let working_dir = file_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let executor = ComposeExecutor::new(&project)
        .await?
        .with_working_dir(&working_dir);

    executor.up(&compose, detach).await?;

    println!("Started {} services", compose.services.len());

    Ok(())
}

/// Convert compose to K8s and apply via API
async fn run_compose_convert(
    file_path: &Path,
    content: &str,
    namespace: &str,
) -> Result<()> {
    let compose = ComposeFile::parse(content)
        .context("Failed to parse compose file")?;

    let app_name = derive_project_name(file_path);
    let translator = ComposeTranslator::new(namespace, &app_name);
    let resources = translator.translate(&compose);

    let api_url = std::env::var("K1S_API_SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:6443".to_string());

    let client = reqwest::Client::new();

    for cm in &resources.config_maps {
        apply_resource(&client, &api_url, namespace, "configmaps", &cm.metadata.name, cm).await?;
    }

    for secret in &resources.secrets {
        apply_resource(&client, &api_url, namespace, "secrets", &secret.metadata.name, secret).await?;
    }

    for svc in &resources.services {
        apply_resource(&client, &api_url, namespace, "services", &svc.metadata.name, svc).await?;
    }

    for pod in &resources.pods {
        apply_resource(&client, &api_url, namespace, "pods", &pod.metadata.name, pod).await?;
    }

    for deployment in &resources.deployments {
        apply_resource(&client, &api_url, namespace, "deployments", &deployment.metadata.name, deployment).await?;
    }

    for ds in &resources.daemon_sets {
        apply_resource(&client, &api_url, namespace, "daemonsets", &ds.metadata.name, ds).await?;
    }

    Ok(())
}

/// Apply a Kubernetes manifest to the API server
async fn run_kubernetes_apply(content: &str, default_namespace: &str) -> Result<()> {
    let api_url = std::env::var("K1S_API_SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:6443".to_string());

    let client = reqwest::Client::new();

    for doc in content.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        let manifest: Value = serde_yaml::from_str(doc)
            .context("Failed to parse YAML document")?;

        let kind = manifest["kind"].as_str().unwrap_or("Unknown");
        let name = manifest["metadata"]["name"].as_str().unwrap_or("unknown");
        let namespace = manifest["metadata"]["namespace"]
            .as_str()
            .unwrap_or(default_namespace);

        let url = build_api_url(&api_url, kind, namespace);

        let response = client.post(&url).json(&manifest).send().await?;

        if response.status().is_success() {
            println!("{}/{} created", kind.to_lowercase(), name);
        } else if response.status().as_u16() == 409 {
            println!("{}/{} configured (already exists)", kind.to_lowercase(), name);
        } else {
            let error = response.text().await?;
            println!("Error creating {}/{}: {}", kind.to_lowercase(), name, error);
        }
    }

    Ok(())
}

/// Run compose up command
pub async fn run_up(args: UpArgs) -> Result<()> {
    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("Failed to read {}", args.file.display()))?;

    run_compose_native(&args.file, &content, args.project_name, args.detach).await
}

/// Run compose down command
pub async fn run_down(args: DownArgs) -> Result<()> {
    let content = std::fs::read_to_string(&args.file)
        .with_context(|| format!("Failed to read {}", args.file.display()))?;

    let compose = ComposeFile::parse(&content)
        .context("Failed to parse compose file")?;

    let project = args.project_name.unwrap_or_else(|| derive_project_name(&args.file));

    let working_dir = args
        .file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let executor = ComposeExecutor::new(&project)
        .await?
        .with_working_dir(&working_dir);

    executor.down(&compose, args.volumes).await?;

    println!("Stopped services for project {}", project);

    Ok(())
}

/// Run ps command for compose containers
pub async fn run_ps(args: PsArgs) -> Result<()> {
    let project = args.project_name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".to_string())
    });

    let executor = ComposeExecutor::new(&project).await?;
    let containers = executor.ps().await?;

    if containers.is_empty() {
        println!("No containers found for project {}", project);
        return Ok(());
    }

    println!(
        "{:<12} {:<30} {:<20} {:<10} {:<30}",
        "CONTAINER ID", "NAME", "IMAGE", "STATE", "PORTS"
    );

    for c in containers {
        println!(
            "{:<12} {:<30} {:<20} {:<10} {:<30}",
            c.id, c.name, c.image, c.state, c.ports
        );
    }

    Ok(())
}

async fn apply_resource<T: serde::Serialize>(
    client: &reqwest::Client,
    api_url: &str,
    namespace: &str,
    resource_type: &str,
    name: &str,
    resource: &T,
) -> Result<()> {
    let url = format!("{}/api/v1/namespaces/{}/{}", api_url, namespace, resource_type);

    match client.post(&url).json(resource).send().await {
        Ok(resp) if resp.status().is_success() => {
            println!("{}/{} created", resource_type.trim_end_matches('s'), name);
        }
        Ok(resp) if resp.status().as_u16() == 409 => {
            println!("{}/{} configured", resource_type.trim_end_matches('s'), name);
        }
        Ok(resp) => {
            println!("{}/{} failed: {}", resource_type.trim_end_matches('s'), name, resp.status());
        }
        Err(e) => {
            println!("{}/{} error: {}", resource_type.trim_end_matches('s'), name, e);
        }
    }

    Ok(())
}

fn build_api_url(api_url: &str, kind: &str, namespace: &str) -> String {
    match kind.to_lowercase().as_str() {
        "namespace" => format!("{}/api/v1/namespaces", api_url),
        "node" => format!("{}/api/v1/nodes", api_url),
        "pod" => format!("{}/api/v1/namespaces/{}/pods", api_url, namespace),
        "service" => format!("{}/api/v1/namespaces/{}/services", api_url, namespace),
        "configmap" => format!("{}/api/v1/namespaces/{}/configmaps", api_url, namespace),
        "secret" => format!("{}/api/v1/namespaces/{}/secrets", api_url, namespace),
        "deployment" => format!("{}/apis/apps/v1/namespaces/{}/deployments", api_url, namespace),
        "daemonset" => format!("{}/apis/apps/v1/namespaces/{}/daemonsets", api_url, namespace),
        _ => format!("{}/api/v1/namespaces/{}/{}", api_url, namespace, kind.to_lowercase()),
    }
}

fn derive_project_name(file_path: &Path) -> String {
    file_path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_compose_file() {
        let compose = r#"
version: "3"
services:
  web:
    image: nginx
"#;
        assert_eq!(detect_file_type(compose), FileType::Compose);
    }

    #[test]
    fn test_detect_kubernetes_pod() {
        let k8s = r#"
apiVersion: v1
kind: Pod
metadata:
  name: test
spec:
  containers:
    - name: test
      image: nginx
"#;
        assert_eq!(detect_file_type(k8s), FileType::Kubernetes);
    }

    #[test]
    fn test_detect_kubernetes_multi_doc() {
        let k8s = r#"
---
apiVersion: v1
kind: Service
metadata:
  name: test
---
apiVersion: v1
kind: Pod
metadata:
  name: test
"#;
        assert_eq!(detect_file_type(k8s), FileType::Kubernetes);
    }

    #[test]
    fn test_detect_compose_with_version() {
        let compose = r#"
version: "3.8"
services:
  api:
    image: myapp:latest
    deploy:
      replicas: 3
"#;
        assert_eq!(detect_file_type(compose), FileType::Compose);
    }

    #[test]
    fn test_derive_project_name() {
        let path = Path::new("/home/user/myproject/docker-compose.yml");
        assert_eq!(derive_project_name(path), "myproject");

        let path = Path::new("docker-compose.yml");
        assert_eq!(derive_project_name(path), "app");
    }
}
