//! Agent command - starts worker node only

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use tracing::info;

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

    let node_name = args.node_name.unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "k1s-agent".to_string())
    });

    info!("Node name: {}", node_name);

    // TODO: Connect to API server
    // TODO: Register node
    // TODO: Start kubelet
    // TODO: Start kube-proxy

    if args.p2p {
        info!("P2P mode enabled on {}", args.p2p_address);
        // TODO: Start P2P networking and connect to cluster
    }

    // Keep running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}
