//! Server command - starts control plane and worker node

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use tracing::{error, info};

use k1s_api_server::{ApiServer, server::ApiServerConfig};
use k1s_controller_manager::ControllerManager;
use k1s_kubelet::{Kubelet, KubeletConfig, RuntimeConfig};
use k1s_scheduler::{Scheduler, SchedulerConfig};

#[derive(Args)]
pub struct ServerArgs {
    /// Address to bind the API server
    #[arg(long, default_value = "0.0.0.0:6443", env = "K1S_BIND_ADDRESS")]
    pub bind_address: SocketAddr,

    /// Data directory for storage
    #[arg(long, default_value = "/var/lib/k1s", env = "K1S_DATA_DIR")]
    pub data_dir: PathBuf,

    /// Node name (defaults to hostname)
    #[arg(long, env = "K1S_NODE_NAME")]
    pub node_name: Option<String>,

    /// Disable worker node functionality (control plane only)
    #[arg(long)]
    pub disable_agent: bool,

    /// Container runtime to use (docker, containerd)
    #[arg(long, default_value = "docker", env = "K1S_CONTAINER_RUNTIME")]
    pub container_runtime: String,

    /// Container runtime socket path
    #[arg(long, env = "K1S_RUNTIME_SOCKET")]
    pub runtime_socket: Option<String>,

    /// Enable P2P mode for multi-node clusters
    #[arg(long)]
    pub p2p: bool,

    /// P2P listen address
    #[arg(long, default_value = "0.0.0.0:4001")]
    pub p2p_address: SocketAddr,

    /// Bootstrap peers for P2P (comma-separated)
    #[arg(long, env = "K1S_P2P_PEERS")]
    pub p2p_peers: Option<String>,

    /// Disable TLS (insecure, for testing only)
    #[arg(long)]
    pub disable_tls: bool,

    /// Write kubeconfig to this path
    #[arg(long, env = "K1S_WRITE_KUBECONFIG")]
    pub write_kubeconfig: Option<PathBuf>,
}

pub async fn run(args: ServerArgs) -> Result<()> {
    info!("Starting k1s server");
    info!("Bind address: {}", args.bind_address);
    info!("Data directory: {}", args.data_dir.display());
    info!("Container runtime: {}", args.container_runtime);

    let node_name = args.node_name.clone().unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "k1s-node".to_string())
    });

    info!("Node name: {}", node_name);

    // Create API server config
    let config = ApiServerConfig {
        bind_address: args.bind_address,
        data_dir: args.data_dir.clone(),
        node_name: node_name.clone(),
        tls_enabled: !args.disable_tls,
    };

    let server = ApiServer::new(config)?;

    // Write kubeconfig if requested
    if let Some(ref kubeconfig_path) = args.write_kubeconfig {
        if let Some(kubeconfig) = server.kubeconfig() {
            if let Some(parent) = kubeconfig_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(kubeconfig_path, &kubeconfig)?;
            info!("Wrote kubeconfig to {}", kubeconfig_path.display());
        }
    } else if server.tls_certs().is_some() {
        // Auto-write to data_dir
        let kubeconfig_path = args.data_dir.join("k1s.yaml");
        if let Some(kubeconfig) = server.kubeconfig() {
            std::fs::write(&kubeconfig_path, &kubeconfig)?;
            info!("Wrote kubeconfig to {}", kubeconfig_path.display());
        }
    }

    // Initialize default resources
    server.init_defaults().await?;

    // Register this node
    register_node(&server, &args).await?;

    // Get storage for other components
    let storage = server.storage();

    // Start scheduler
    let scheduler_storage = storage.clone();
    let scheduler_handle = tokio::spawn(async move {
        let scheduler = Scheduler::with_config(
            scheduler_storage,
            SchedulerConfig::default(),
        );
        if let Err(e) = scheduler.run().await {
            error!("Scheduler error: {}", e);
        }
    });
    info!("Scheduler started");

    // Start controller manager
    let controller_storage = storage.clone();
    let controller_handle = tokio::spawn(async move {
        let manager = ControllerManager::new(controller_storage);
        manager.run().await;
    });
    info!("Controller manager started");

    // Start kubelet if agent is enabled
    let kubelet_handle = if !args.disable_agent {
        let kubelet_storage = storage.clone();
        let kubelet_node_name = node_name.clone();
        let runtime_type = args.container_runtime.clone();
        let socket_path = args.runtime_socket.clone().unwrap_or_else(|| {
            match runtime_type.as_str() {
                "containerd" => "/run/containerd/containerd.sock".to_string(),
                _ => "/var/run/docker.sock".to_string(),
            }
        });

        let handle = tokio::spawn(async move {
            let kubelet_config = KubeletConfig {
                node_name: kubelet_node_name,
                runtime: RuntimeConfig {
                    runtime_type,
                    socket_path,
                },
                ..Default::default()
            };

            match Kubelet::new(kubelet_config, kubelet_storage).await {
                Ok(kubelet) => {
                    if let Err(e) = kubelet.run().await {
                        error!("Kubelet error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to start kubelet: {}", e);
                }
            }
        });
        info!("Kubelet started");
        Some(handle)
    } else {
        info!("Agent disabled, kubelet not started");
        None
    };

    // Start P2P if enabled
    if args.p2p {
        info!("P2P mode enabled on {}", args.p2p_address);
        // TODO: Start P2P networking
    }

    // Run the API server (blocking)
    let result = server.run().await;

    // Cleanup
    scheduler_handle.abort();
    controller_handle.abort();
    if let Some(handle) = kubelet_handle {
        handle.abort();
    }

    result
}

async fn register_node(server: &ApiServer, args: &ServerArgs) -> Result<()> {
    use k1s_storage::backend::ResourceStore;
    use k1s_types::{
        Node, NodeAddress, NodeAddressType, NodeCondition, NodeConditionType, NodeDaemonEndpoints,
        NodeSpec, NodeStatus, NodeSystemInfo, DaemonEndpoint,
    };
    use std::collections::BTreeMap;

    let node_name = args.node_name.clone().unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "k1s-node".to_string())
    });

    let node_store = ResourceStore::<Node>::new(server.storage());

    // Check if node already exists
    if node_store.get(None, &node_name).await?.is_some() {
        info!("Node {} already registered", node_name);
        return Ok(());
    }

    // Create node resource
    let mut node = Node::new(&node_name);

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
                address: node_name.clone(),
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

    node_store.create(node).await?;
    info!("Registered node: {}", node_name);

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
    // Simple method to get local IP
    "127.0.0.1".to_string()
}
