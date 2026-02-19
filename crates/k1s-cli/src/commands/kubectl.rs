//! kubectl-compatible commands
//!
//! Provides kubectl-style interface for interacting with the k1s API server.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::Value;
use tracing::info;

fn api_url() -> String {
    std::env::var("K1S_API_SERVER").unwrap_or_else(|_| "http://127.0.0.1:6443".to_string())
}

/// Simple URL encoding for query parameters
fn encode_uri_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}

#[derive(Args)]
pub struct GetArgs {
    /// Resource type (pods, services, deployments, etc.)
    pub resource: String,

    /// Resource name (optional)
    pub name: Option<String>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// All namespaces
    #[arg(short = 'A', long)]
    pub all_namespaces: bool,

    /// Output format (json, yaml, wide)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Label selector
    #[arg(short, long)]
    pub selector: Option<String>,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Resource type
    pub resource: Option<String>,

    /// Resource name
    pub name: Option<String>,

    /// Create from file
    #[arg(short, long)]
    pub filename: Option<PathBuf>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Resource type
    pub resource: String,

    /// Resource name
    pub name: Option<String>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Delete from file
    #[arg(short, long)]
    pub filename: Option<PathBuf>,

    /// Force deletion
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct RunArgs {
    /// Name of the pod
    pub name: String,

    /// Container image
    #[arg(long)]
    pub image: String,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Command to run
    #[arg(long)]
    pub command: Option<String>,

    /// Environment variables (KEY=VALUE)
    #[arg(long)]
    pub env: Vec<String>,

    /// Port to expose
    #[arg(long)]
    pub port: Option<i32>,

    /// Labels (key=value)
    #[arg(short, long)]
    pub labels: Vec<String>,

    /// Delete pod after it exits
    #[arg(long)]
    pub rm: bool,

    /// Interactive mode
    #[arg(short, long)]
    pub stdin: bool,

    /// Allocate TTY
    #[arg(short, long)]
    pub tty: bool,
}

#[derive(Args)]
pub struct DescribeArgs {
    /// Resource type
    pub resource: String,

    /// Resource name
    pub name: Option<String>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,
}

#[derive(Args)]
pub struct LogsArgs {
    /// Pod name
    pub pod: String,

    /// Container name
    #[arg(short, long)]
    pub container: Option<String>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Follow logs
    #[arg(short, long)]
    pub follow: bool,

    /// Number of lines
    #[arg(long)]
    pub tail: Option<i32>,

    /// Show previous container logs
    #[arg(short, long)]
    pub previous: bool,
}

#[derive(Args)]
pub struct ExecArgs {
    /// Pod name
    pub pod: String,

    /// Command to execute
    pub command: Vec<String>,

    /// Container name
    #[arg(short, long)]
    pub container: Option<String>,

    /// Namespace
    #[arg(short, long, default_value = "default")]
    pub namespace: String,

    /// Interactive mode
    #[arg(short, long)]
    pub stdin: bool,

    /// Allocate TTY
    #[arg(short, long)]
    pub tty: bool,
}

pub async fn get_resources(args: GetArgs) -> Result<()> {
    let api_url = api_url();
    let client = reqwest::Client::new();

    let url = if args.all_namespaces {
        format!("{}/api/v1/{}", api_url, args.resource)
    } else if let Some(name) = &args.name {
        format!(
            "{}/api/v1/namespaces/{}/{}/{}",
            api_url, args.namespace, args.resource, name
        )
    } else {
        format!(
            "{}/api/v1/namespaces/{}/{}",
            api_url, args.namespace, args.resource
        )
    };

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        anyhow::bail!("API error ({}): {}", status, body);
    }

    let body: Value = response.json().await?;

    match args.output.as_deref() {
        Some("json") => {
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        Some("yaml") => {
            println!("{}", serde_yaml::to_string(&body)?);
        }
        _ => {
            print_resource_table(&args.resource, &body)?;
        }
    }

    Ok(())
}

fn print_resource_table(resource: &str, data: &Value) -> Result<()> {
    let empty = vec![];
    let single = vec![data.clone()];
    let items = if data.get("items").is_some() {
        data["items"].as_array().unwrap_or(&empty)
    } else {
        &single
    };

    match resource {
        "pods" | "pod" | "po" => {
            println!(
                "{:<20} {:<10} {:<10} {:<8} {:<10}",
                "NAME", "READY", "STATUS", "RESTARTS", "AGE"
            );
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                let status = item["status"]["phase"].as_str().unwrap_or("Unknown");
                println!(
                    "{:<20} {:<10} {:<10} {:<8} {:<10}",
                    name, "0/1", status, "0", "-"
                );
            }
        }
        "services" | "service" | "svc" => {
            println!(
                "{:<20} {:<12} {:<15} {:<20} {:<10}",
                "NAME", "TYPE", "CLUSTER-IP", "EXTERNAL-IP", "PORT(S)"
            );
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                let svc_type = item["spec"]["type"].as_str().unwrap_or("ClusterIP");
                let cluster_ip = item["spec"]["clusterIP"].as_str().unwrap_or("<none>");
                println!(
                    "{:<20} {:<12} {:<15} {:<20} {:<10}",
                    name, svc_type, cluster_ip, "<none>", "-"
                );
            }
        }
        "namespaces" | "namespace" | "ns" => {
            println!("{:<20} {:<10} {:<10}", "NAME", "STATUS", "AGE");
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                let status = item["status"]["phase"].as_str().unwrap_or("Active");
                println!("{:<20} {:<10} {:<10}", name, status, "-");
            }
        }
        "nodes" | "node" | "no" => {
            println!(
                "{:<20} {:<10} {:<20} {:<10}",
                "NAME", "STATUS", "ROLES", "VERSION"
            );
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                println!("{:<20} {:<10} {:<20} {:<10}", name, "Ready", "control-plane", "v0.1.0");
            }
        }
        "configmaps" | "configmap" | "cm" => {
            println!("{:<30} {:<10} {:<10}", "NAME", "DATA", "AGE");
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                let data_count = item["data"].as_object().map(|o| o.len()).unwrap_or(0);
                println!("{:<30} {:<10} {:<10}", name, data_count, "-");
            }
        }
        "secrets" | "secret" => {
            println!(
                "{:<30} {:<25} {:<10} {:<10}",
                "NAME", "TYPE", "DATA", "AGE"
            );
            for item in items {
                let name = item["metadata"]["name"].as_str().unwrap_or("-");
                let secret_type = item["type"].as_str().unwrap_or("Opaque");
                let data_count = item["data"].as_object().map(|o| o.len()).unwrap_or(0);
                println!(
                    "{:<30} {:<25} {:<10} {:<10}",
                    name, secret_type, data_count, "-"
                );
            }
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(data)?);
        }
    }

    Ok(())
}

pub async fn create_resource(args: CreateArgs) -> Result<()> {
    let api_url = api_url();

    if let Some(filename) = args.filename {
        let content = std::fs::read_to_string(&filename)?;
        apply_manifest(&api_url, &content, &args.namespace).await?;
    } else if let (Some(resource), Some(name)) = (args.resource, args.name) {
        match resource.as_str() {
            "namespace" | "ns" => {
                let ns = serde_json::json!({
                    "apiVersion": "v1",
                    "kind": "Namespace",
                    "metadata": { "name": name }
                });
                let client = reqwest::Client::new();
                let url = format!("{}/api/v1/namespaces", api_url);
                let response = client.post(&url).json(&ns).send().await?;
                if response.status().is_success() {
                    println!("namespace/{} created", name);
                } else {
                    anyhow::bail!("Failed to create namespace: {}", response.text().await?);
                }
            }
            _ => {
                anyhow::bail!(
                    "Cannot create {} without a file. Use -f to specify a file.",
                    resource
                );
            }
        }
    } else {
        anyhow::bail!("Must specify either a filename (-f) or resource type and name");
    }

    Ok(())
}

async fn apply_manifest(api_url: &str, content: &str, default_namespace: &str) -> Result<()> {
    for doc in content.split("---") {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }

        let manifest: Value = serde_yaml::from_str(doc)?;
        let kind = manifest["kind"].as_str().unwrap_or("Unknown");
        let name = manifest["metadata"]["name"].as_str().unwrap_or("unknown");
        let namespace = manifest["metadata"]["namespace"]
            .as_str()
            .unwrap_or(default_namespace);

        let url = match kind.to_lowercase().as_str() {
            "namespace" => format!("{}/api/v1/namespaces", api_url),
            "node" => format!("{}/api/v1/nodes", api_url),
            "pod" => format!("{}/api/v1/namespaces/{}/pods", api_url, namespace),
            "service" => format!("{}/api/v1/namespaces/{}/services", api_url, namespace),
            "configmap" => format!("{}/api/v1/namespaces/{}/configmaps", api_url, namespace),
            "secret" => format!("{}/api/v1/namespaces/{}/secrets", api_url, namespace),
            "deployment" => {
                format!(
                    "{}/apis/apps/v1/namespaces/{}/deployments",
                    api_url, namespace
                )
            }
            "daemonset" => {
                format!(
                    "{}/apis/apps/v1/namespaces/{}/daemonsets",
                    api_url, namespace
                )
            }
            _ => {
                println!("Unsupported resource kind: {}", kind);
                continue;
            }
        };

        let client = reqwest::Client::new();
        let response = client.post(&url).json(&manifest).send().await?;

        if response.status().is_success() {
            println!("{}/{} created", kind.to_lowercase(), name);
        } else if response.status().as_u16() == 409 {
            println!(
                "{}/{} configured (already exists)",
                kind.to_lowercase(),
                name
            );
        } else {
            let error = response.text().await?;
            println!("Error creating {}/{}: {}", kind.to_lowercase(), name, error);
        }
    }

    Ok(())
}

pub async fn delete_resource(args: DeleteArgs) -> Result<()> {
    let api_url = api_url();
    let client = reqwest::Client::new();

    if let Some(filename) = args.filename {
        let content = std::fs::read_to_string(&filename)?;
        for doc in content.split("---") {
            let doc = doc.trim();
            if doc.is_empty() {
                continue;
            }

            let manifest: Value = serde_yaml::from_str(doc)?;
            let kind = manifest["kind"]
                .as_str()
                .unwrap_or("Unknown")
                .to_lowercase();
            let name = manifest["metadata"]["name"].as_str().unwrap_or("unknown");
            let namespace = manifest["metadata"]["namespace"]
                .as_str()
                .unwrap_or(&args.namespace);

            let url = build_resource_url(&api_url, &kind, Some(name), namespace);
            let response = client.delete(&url).send().await?;

            if response.status().is_success() {
                println!("{}/{} deleted", kind, name);
            } else {
                println!("Error deleting {}/{}", kind, name);
            }
        }
    } else if let Some(name) = args.name {
        let url = build_resource_url(&api_url, &args.resource, Some(&name), &args.namespace);
        let response = client.delete(&url).send().await?;

        if response.status().is_success() {
            println!("{}/{} deleted", args.resource, name);
        } else {
            anyhow::bail!("Failed to delete: {}", response.text().await?);
        }
    } else {
        anyhow::bail!("Must specify resource name or filename");
    }

    Ok(())
}

fn build_resource_url(api_url: &str, resource: &str, name: Option<&str>, namespace: &str) -> String {
    let resource_lower = resource.to_lowercase();
    let plural = match resource_lower.as_str() {
        "pod" | "po" => "pods",
        "service" | "svc" => "services",
        "namespace" | "ns" => "namespaces",
        "node" | "no" => "nodes",
        "configmap" | "cm" => "configmaps",
        "secret" => "secrets",
        "deployment" => "deployments",
        "daemonset" => "daemonsets",
        _ => &resource_lower,
    };

    let is_namespaced = !matches!(plural, "namespaces" | "nodes");

    match (is_namespaced, name) {
        (true, Some(n)) => format!(
            "{}/api/v1/namespaces/{}/{}/{}",
            api_url, namespace, plural, n
        ),
        (true, None) => format!("{}/api/v1/namespaces/{}/{}", api_url, namespace, plural),
        (false, Some(n)) => format!("{}/api/v1/{}/{}", api_url, plural, n),
        (false, None) => format!("{}/api/v1/{}", api_url, plural),
    }
}

pub async fn run_pod(args: RunArgs) -> Result<()> {
    use k1s_types::{Container, ContainerPort, EnvVar, Pod, PodSpec};
    use std::collections::BTreeMap;

    let api_url = api_url();

    let mut labels = BTreeMap::new();
    labels.insert("run".to_string(), args.name.clone());

    for label in &args.labels {
        if let Some((k, v)) = label.split_once('=') {
            labels.insert(k.to_string(), v.to_string());
        }
    }

    let env: Vec<EnvVar> = args
        .env
        .iter()
        .filter_map(|e| {
            e.split_once('=').map(|(k, v)| EnvVar {
                name: k.to_string(),
                value: Some(v.to_string()),
                value_from: None,
            })
        })
        .collect();

    let ports: Vec<ContainerPort> = args
        .port
        .map(|p| {
            vec![ContainerPort {
                container_port: p,
                ..Default::default()
            }]
        })
        .unwrap_or_default();

    let container = Container {
        name: args.name.clone(),
        image: args.image.clone(),
        env,
        ports,
        stdin: args.stdin,
        tty: args.tty,
        ..Default::default()
    };

    let pod = Pod {
        metadata: k1s_types::ObjectMeta {
            name: args.name.clone(),
            namespace: Some(args.namespace.clone()),
            labels,
            ..Default::default()
        },
        spec: Some(PodSpec {
            containers: vec![container],
            ..Default::default()
        }),
        ..Default::default()
    };

    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/namespaces/{}/pods", api_url, args.namespace);
    let response = client.post(&url).json(&pod).send().await?;

    if response.status().is_success() {
        println!("pod/{} created", args.name);
    } else {
        anyhow::bail!("Failed to create pod: {}", response.text().await?);
    }

    Ok(())
}

pub async fn describe_resource(args: DescribeArgs) -> Result<()> {
    info!(
        "Describing {} in namespace {}",
        args.resource, args.namespace
    );
    println!("(describe not yet implemented)");
    Ok(())
}

pub async fn get_logs(args: LogsArgs) -> Result<()> {
    let api_url = api_url();
    let client = reqwest::Client::new();

    // Build query parameters
    let mut query_params = vec![];
    if let Some(container) = &args.container {
        query_params.push(format!("container={}", container));
    }
    if let Some(tail) = args.tail {
        query_params.push(format!("tailLines={}", tail));
    }
    if args.follow {
        query_params.push("follow=true".to_string());
    }

    let query_string = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    let url = format!(
        "{}/api/v1/namespaces/{}/pods/{}/log{}",
        api_url, args.namespace, args.pod, query_string
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        anyhow::bail!("Failed to get logs ({}): {}", status, body);
    }

    let logs = response.text().await?;
    print!("{}", logs);

    Ok(())
}

pub async fn exec_command(args: ExecArgs) -> Result<()> {
    let api_url = api_url();
    let client = reqwest::Client::new();

    if args.command.is_empty() {
        anyhow::bail!("No command specified. Use: k1s exec <pod> -- <command>");
    }

    // Build query parameters
    let mut query_params = vec![];
    if let Some(container) = &args.container {
        query_params.push(format!("container={}", container));
    }
    for cmd in &args.command {
        query_params.push(format!("command={}", encode_uri_component(cmd)));
    }

    let query_string = format!("?{}", query_params.join("&"));

    let url = format!(
        "{}/api/v1/namespaces/{}/pods/{}/exec{}",
        api_url, args.namespace, args.pod, query_string
    );

    let response = client.post(&url).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;
        anyhow::bail!("Failed to exec ({}): {}", status, body);
    }

    let result: serde_json::Value = response.json().await?;

    // Print stdout
    if let Some(stdout) = result.get("stdout").and_then(|v| v.as_str()) {
        print!("{}", stdout);
    }

    // Print stderr to stderr
    if let Some(stderr) = result.get("stderr").and_then(|v| v.as_str()) {
        if !stderr.is_empty() {
            eprint!("{}", stderr);
        }
    }

    // Exit with the command's exit code
    if let Some(exit_code) = result.get("exitCode").and_then(|v| v.as_i64()) {
        if exit_code != 0 {
            std::process::exit(exit_code as i32);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_resource_url_namespaced() {
        let url = build_resource_url("http://localhost:6443", "pod", Some("test"), "default");
        assert_eq!(url, "http://localhost:6443/api/v1/namespaces/default/pods/test");
    }

    #[test]
    fn test_build_resource_url_cluster_scoped() {
        let url = build_resource_url("http://localhost:6443", "namespace", Some("test"), "default");
        assert_eq!(url, "http://localhost:6443/api/v1/namespaces/test");
    }

    #[test]
    fn test_build_resource_url_aliases() {
        let url = build_resource_url("http://localhost:6443", "po", Some("test"), "default");
        assert_eq!(url, "http://localhost:6443/api/v1/namespaces/default/pods/test");

        let url = build_resource_url("http://localhost:6443", "svc", Some("test"), "default");
        assert_eq!(url, "http://localhost:6443/api/v1/namespaces/default/services/test");
    }
}
