//! k1s CLI - Lightweight Kubernetes distribution
//!
//! A single binary that supports:
//! - Kubernetes server and agent modes
//! - Native Docker Compose execution
//! - kubectl-compatible commands

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;
mod compose;

use commands::{agent, apply, kubectl, server};
use compose::{ComposeFile, ComposeTranslator};

#[derive(Parser)]
#[command(name = "k1s")]
#[command(author, version, about = "Lightweight Kubernetes distribution written in Rust")]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Log output format (text, json)
    #[arg(long, global = true, default_value = "text")]
    log_format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // === Server/Agent modes ===
    /// Start k1s server (control plane + worker node)
    Server(server::ServerArgs),

    /// Start k1s agent (worker node only)
    Agent(agent::AgentArgs),

    // === Unified apply (auto-detects K8s or Compose) ===
    /// Apply configuration (auto-detects Kubernetes manifests or Compose files)
    Apply(apply::ApplyArgs),

    // === Compose-style commands ===
    /// Start compose services (docker-compose up)
    Up(apply::UpArgs),

    /// Stop compose services (docker-compose down)
    Down(apply::DownArgs),

    /// List compose containers (docker-compose ps)
    Ps(apply::PsArgs),

    // === kubectl-compatible commands ===
    /// kubectl-compatible CLI (e.g., k1s kubectl get pods)
    #[command(subcommand)]
    Kubectl(KubectlCommands),

    /// docker-compose compatible CLI (e.g., k1s compose up)
    #[command(subcommand)]
    Compose(ComposeCommands),

    /// Get resources
    Get(kubectl::GetArgs),

    /// Create resources
    Create(kubectl::CreateArgs),

    /// Delete resources
    Delete(kubectl::DeleteArgs),

    /// Run a container
    Run(kubectl::RunArgs),

    /// Describe resources
    Describe(kubectl::DescribeArgs),

    /// View logs
    Logs(kubectl::LogsArgs),

    /// Execute command in container
    Exec(kubectl::ExecArgs),

    // === Utility commands ===
    /// Container runtime CLI commands
    #[command(subcommand)]
    Cri(CriCommands),

    /// Convert docker-compose.yml to Kubernetes manifests
    Convert(ConvertArgs),
}

/// kubectl-compatible subcommands
#[derive(Subcommand)]
enum KubectlCommands {
    /// Get resources
    Get(kubectl::GetArgs),
    /// Create resources
    Create(kubectl::CreateArgs),
    /// Delete resources
    Delete(kubectl::DeleteArgs),
    /// Run a container
    Run(kubectl::RunArgs),
    /// Describe resources
    Describe(kubectl::DescribeArgs),
    /// View logs
    Logs(kubectl::LogsArgs),
    /// Execute command in container
    Exec(kubectl::ExecArgs),
    /// Apply configuration
    Apply(apply::ApplyArgs),
}

/// docker-compose compatible subcommands
#[derive(Subcommand)]
enum ComposeCommands {
    /// Start services
    Up(apply::UpArgs),
    /// Stop services
    Down(apply::DownArgs),
    /// List containers
    Ps(apply::PsArgs),
    /// Convert to Kubernetes manifests
    Convert(ConvertArgs),
}

/// Convert compose file to K8s manifests
#[derive(Args)]
struct ConvertArgs {
    /// Path to docker-compose.yml file
    #[arg(short, long, default_value = "docker-compose.yml")]
    file: PathBuf,

    /// Output format (yaml, json)
    #[arg(short, long, default_value = "yaml")]
    output: String,

    /// Namespace for generated resources
    #[arg(short, long, default_value = "default")]
    namespace: String,

    /// Application name prefix
    #[arg(short, long)]
    app_name: Option<String>,
}

#[derive(Subcommand)]
enum CriCommands {
    /// List running containers
    Containers,
    /// List images
    Images,
}

fn init_logging(verbose: bool, format: &str) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging(cli.verbose, &cli.log_format);

    match cli.command {
        // Server/Agent modes
        Commands::Server(args) => server::run(args).await,
        Commands::Agent(args) => agent::run(args).await,

        // Unified apply
        Commands::Apply(args) => apply::run_apply(args).await,

        // Compose-style commands
        Commands::Up(args) => apply::run_up(args).await,
        Commands::Down(args) => apply::run_down(args).await,
        Commands::Ps(args) => apply::run_ps(args).await,

        // kubectl subcommand
        Commands::Kubectl(cmd) => run_kubectl(cmd).await,

        // compose subcommand
        Commands::Compose(cmd) => run_compose(cmd).await,

        // kubectl-compatible commands (also at top level)
        Commands::Get(args) => kubectl::get_resources(args).await,
        Commands::Create(args) => kubectl::create_resource(args).await,
        Commands::Delete(args) => kubectl::delete_resource(args).await,
        Commands::Run(args) => kubectl::run_pod(args).await,
        Commands::Describe(args) => kubectl::describe_resource(args).await,
        Commands::Logs(args) => kubectl::get_logs(args).await,
        Commands::Exec(args) => kubectl::exec_command(args).await,

        // Utility commands
        Commands::Cri(cmd) => run_cri(cmd).await,
        Commands::Convert(args) => run_convert(args).await,
    }
}

async fn run_kubectl(cmd: KubectlCommands) -> Result<()> {
    match cmd {
        KubectlCommands::Get(args) => kubectl::get_resources(args).await,
        KubectlCommands::Create(args) => kubectl::create_resource(args).await,
        KubectlCommands::Delete(args) => kubectl::delete_resource(args).await,
        KubectlCommands::Run(args) => kubectl::run_pod(args).await,
        KubectlCommands::Describe(args) => kubectl::describe_resource(args).await,
        KubectlCommands::Logs(args) => kubectl::get_logs(args).await,
        KubectlCommands::Exec(args) => kubectl::exec_command(args).await,
        KubectlCommands::Apply(args) => apply::run_apply(args).await,
    }
}

async fn run_compose(cmd: ComposeCommands) -> Result<()> {
    match cmd {
        ComposeCommands::Up(args) => apply::run_up(args).await,
        ComposeCommands::Down(args) => apply::run_down(args).await,
        ComposeCommands::Ps(args) => apply::run_ps(args).await,
        ComposeCommands::Convert(args) => run_convert(args).await,
    }
}

async fn run_cri(cmd: CriCommands) -> Result<()> {
    match cmd {
        CriCommands::Containers => {
            info!("Listing containers...");
            println!(
                "{:<12} {:<20} {:<15} {:<10} {:<15}",
                "CONTAINER ID", "IMAGE", "COMMAND", "STATUS", "NAMES"
            );
        }
        CriCommands::Images => {
            info!("Listing images...");
            println!(
                "{:<40} {:<15} {:<15} {:<10}",
                "REPOSITORY", "TAG", "IMAGE ID", "SIZE"
            );
        }
    }
    Ok(())
}

async fn run_convert(args: ConvertArgs) -> Result<()> {
    let compose = ComposeFile::from_file(&args.file)
        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", args.file.display(), e))?;

    let app_name = args.app_name.unwrap_or_else(|| {
        args.file
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "app".to_string())
    });

    let translator = ComposeTranslator::new(&args.namespace, &app_name);
    let resources = translator.translate(&compose);

    let output = match args.output.as_str() {
        "json" => serde_json::to_string_pretty(&serde_json::json!({
            "configMaps": resources.config_maps,
            "secrets": resources.secrets,
            "services": resources.services,
            "pods": resources.pods,
            "deployments": resources.deployments,
            "daemonSets": resources.daemon_sets,
        }))?,
        _ => resources.to_yaml()?,
    };

    println!("{output}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
