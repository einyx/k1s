//! Agent command - starts worker node only

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Args;
use k1s_client::K1sClient;
use k1s_types::{
    DaemonEndpoint, Node, NodeAddress, NodeAddressType, NodeCondition, NodeConditionType,
    NodeDaemonEndpoints, NodeSpec, NodeStatus, NodeSystemInfo, Pod,
};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

#[derive(Args)]
pub struct AgentArgs {
    /// API server URL to connect to
    #[arg(long, env = "K1S_SERVER")]
    pub server: String,

    /// Token for authentication
    #[arg(long, env = "K1S_TOKEN")]
    pub token: String,

    /// Data directory for storage
    #[arg(long, default_value = "/var/lib/k1s", env = "K1S_DATA_DIR")]
    pub data_dir: PathBuf,

    /// Node name (defaults to hostname)
    #[arg(long, env = "K1S_NODE_NAME")]
    pub node_name: Option<String>,

    /// Container runtime to use (docker, containerd)
    #[arg(long, default_value = "docker", env = "K1S_CONTAINER_RUNTIME")]
    pub container_runtime: String,

    /// Container runtime socket path
    #[arg(long, env = "K1S_RUNTIME_SOCKET")]
    pub runtime_socket: Option<String>,

    /// Enable P2P mode
    #[arg(long)]
    pub p2p: bool,

    /// P2P listen address
    #[arg(long, default_value = "0.0.0.0:4001")]
    pub p2p_address: SocketAddr,
}

pub async fn run(args: AgentArgs) -> Result<()> {
    info!("Starting k1s agent");
    info!("Connecting to server: {}", args.server);
    info!("Container runtime: {}", args.container_runtime);

    let node_name = args.node_name.clone().unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "k1s-agent".to_string())
    });

    info!("Node name: {}", node_name);

    // Create API client
    let client = K1sClient::new(&args.server).with_token(args.token.clone());

    // Test connection
    info!("Testing connection to API server...");
    match test_connection(&client).await {
        Ok(_) => info!("Successfully connected to API server"),
        Err(e) => {
            error!("Failed to connect to API server: {}", e);
            return Err(anyhow::anyhow!("Failed to connect to API server: {}", e));
        }
    }

    // Register node
    register_node(&client, &node_name, &args).await?;

    // Initialize container runtime
    let runtime_type = args.container_runtime.clone();
    let socket_path = args.runtime_socket.clone().unwrap_or_else(|| {
        match runtime_type.as_str() {
            "containerd" => "/run/containerd/containerd.sock".to_string(),
            _ => "/var/run/docker.sock".to_string(),
        }
    });

    let runtime_config = k1s_kubelet::RuntimeConfig {
        runtime_type: runtime_type.clone(),
        socket_path,
    };

    let runtime: Arc<dyn k1s_kubelet::runtime::ContainerRuntime> = match runtime_type.as_str() {
        "containerd" => {
            Arc::new(k1s_kubelet::runtime::containerd::ContainerdRuntime::new(&runtime_config).await?)
        }
        _ => {
            Arc::new(k1s_kubelet::runtime::docker::DockerRuntime::new(&runtime_config).await?)
        }
    };

    info!("Container runtime initialized: {}", runtime.name());

    // Start P2P if enabled
    if args.p2p {
        info!("P2P mode enabled on {}", args.p2p_address);
    }

    // Run the agent loop
    let node_name_clone = node_name.clone();
    let client_clone = client.clone();

    // Heartbeat task - update node status periodically
    let heartbeat_node = node_name.clone();
    let heartbeat_client = client.clone();
    let heartbeat_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(e) = update_node_status(&heartbeat_client, &heartbeat_node).await {
                warn!("Failed to update node status: {}", e);
            }
        }
    });

    // Pod sync loop
    let mut sync_ticker = interval(Duration::from_secs(5));
    loop {
        sync_ticker.tick().await;

        // Get pods assigned to this node
        match client_clone.list_pods_for_node(&node_name_clone).await {
            Ok(pods) => {
                debug!("Found {} pods for node {}", pods.len(), node_name_clone);
                for pod in pods {
                    if let Err(e) = sync_pod(&runtime, &client_clone, &pod).await {
                        warn!(
                            "Failed to sync pod {}/{}: {}",
                            pod.metadata.effective_namespace(),
                            pod.metadata.name,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                warn!("Failed to list pods: {}", e);
            }
        }
    }
}

async fn test_connection(client: &K1sClient) -> Result<()> {
    // Try to list nodes to verify connection
    let _nodes: Vec<Node> = client.list_cluster().await?;
    Ok(())
}

async fn register_node(client: &K1sClient, node_name: &str, args: &AgentArgs) -> Result<()> {
    // Check if node already exists
    match client.get_cluster::<Node>(node_name).await {
        Ok(_) => {
            info!("Node {} already registered, updating status", node_name);
            update_node_status(client, node_name).await?;
            return Ok(());
        }
        Err(k1s_client::ClientError::Api { status: 404, .. }) => {
            // Node doesn't exist, create it
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to check node: {}", e));
        }
    }

    // Create node resource
    let mut node = Node::new(node_name);

    node.spec = Some(NodeSpec {
        pod_cidr: Some("10.42.0.0/24".to_string()),
        pod_cidrs: vec!["10.42.0.0/24".to_string()],
        ..Default::default()
    });

    // Get system info
    let os_info = sys_info();

    let mut capacity = BTreeMap::new();
    capacity.insert("cpu".to_string(), num_cpus::get().to_string());
    capacity.insert("memory".to_string(), format!("{}Ki", os_info.memory_kb));
    capacity.insert("pods".to_string(), "110".to_string());

    let allocatable = capacity.clone();

    node.status = Some(NodeStatus {
        capacity,
        allocatable,
        conditions: vec![NodeCondition {
            r#type: NodeConditionType::Ready,
            status: "True".to_string(),
            reason: Some("KubeletReady".to_string()),
            message: Some("kubelet is posting ready status".to_string()),
            last_heartbeat_time: Some(chrono::Utc::now()),
            last_transition_time: Some(chrono::Utc::now()),
        }],
        addresses: vec![
            NodeAddress {
                r#type: NodeAddressType::InternalIP,
                address: get_local_ip(),
            },
            NodeAddress {
                r#type: NodeAddressType::Hostname,
                address: node_name.to_string(),
            },
        ],
        daemon_endpoints: Some(NodeDaemonEndpoints {
            kubelet_endpoint: Some(DaemonEndpoint { port: 10250 }),
        }),
        node_info: Some(NodeSystemInfo {
            machine_id: os_info.machine_id,
            system_uuid: os_info.system_uuid,
            boot_id: os_info.boot_id,
            kernel_version: os_info.kernel_version,
            os_image: os_info.os_image,
            container_runtime_version: format!("{}://0.1.0", args.container_runtime),
            kubelet_version: "v0.1.0".to_string(),
            kube_proxy_version: "v0.1.0".to_string(),
            operating_system: os_info.os,
            architecture: os_info.arch,
        }),
        ..Default::default()
    });

    client.create_cluster(&node).await?;
    info!("Registered node: {}", node_name);

    Ok(())
}

async fn update_node_status(client: &K1sClient, node_name: &str) -> Result<()> {
    // Get current node
    let mut node: Node = client.get_cluster(node_name).await?;

    // Update heartbeat time
    if let Some(ref mut status) = node.status {
        for condition in &mut status.conditions {
            if matches!(condition.r#type, NodeConditionType::Ready) {
                condition.last_heartbeat_time = Some(chrono::Utc::now());
            }
        }
    }

    client.update_cluster(node_name, &node).await?;
    debug!("Updated node {} heartbeat", node_name);

    Ok(())
}

async fn sync_pod(
    runtime: &Arc<dyn k1s_kubelet::runtime::ContainerRuntime>,
    client: &K1sClient,
    pod: &Pod,
) -> Result<()> {
    // Skip pods being deleted
    if pod.metadata.deletion_timestamp.is_some() {
        debug!(
            "Pod {}/{} is being deleted, skipping",
            pod.metadata.effective_namespace(),
            pod.metadata.name
        );
        return Ok(());
    }

    // Check if pod containers are running
    let spec = match &pod.spec {
        Some(s) => s,
        None => return Ok(()),
    };

    for container in &spec.containers {
        let container_name = format!(
            "k1s_{}_{}_{}",
            pod.metadata.effective_namespace(),
            pod.metadata.name,
            container.name
        );

        // Check if container exists
        let containers = runtime.list_containers(&pod.metadata.uid).await?;
        let exists = containers.iter().any(|c| c.name == container_name);

        if !exists {
            info!(
                "Creating container {} for pod {}/{}",
                container.name,
                pod.metadata.effective_namespace(),
                pod.metadata.name
            );

            // Pull image if needed
            if let Err(e) = runtime.pull_image(&container.image).await {
                warn!("Failed to pull image {}: {}", container.image, e);
            }

            // Create and start container
            let env_config = k1s_kubelet::runtime::ContainerEnvConfig {
                env_vars: container
                    .env
                    .iter()
                    .filter_map(|e| e.value.as_ref().map(|v| format!("{}={}", e.name, v)))
                    .collect(),
                volume_data: Default::default(),
            };

            let container_id = runtime
                .create_container_with_env(pod, &container.name, &env_config)
                .await?;

            runtime.start_container(&container_id).await?;

            info!("Started container {}", container_name);
        }
    }

    Ok(())
}

struct SysInfo {
    machine_id: String,
    system_uuid: String,
    boot_id: String,
    kernel_version: String,
    os_image: String,
    os: String,
    arch: String,
    memory_kb: u64,
}

fn sys_info() -> SysInfo {
    SysInfo {
        machine_id: uuid::Uuid::new_v4().to_string(),
        system_uuid: uuid::Uuid::new_v4().to_string(),
        boot_id: uuid::Uuid::new_v4().to_string(),
        kernel_version: std::env::consts::OS.to_string(),
        os_image: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        memory_kb: 8 * 1024 * 1024, // Default 8GB
    }
}

fn get_local_ip() -> String {
    // Try to get actual local IP
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}
