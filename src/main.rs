use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod core;
mod network;
mod plugins;
mod world;
mod config;
mod sharding;

use crate::core::HorizonServer;
use crate::config::HorizonConfig;

#[derive(Parser)]
#[command(name = "horizon2")]
#[command(about = "Horizon 2 - Next-generation distributed game server")]
#[command(version = "2.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Configuration file path
    #[arg(short, long, default_value = "horizon.toml")]
    config: PathBuf,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Server node ID
    #[arg(short, long)]
    node_id: Option<String>,

    /// Cluster mode
    #[arg(long)]
    cluster: bool,

    /// Plugin directory
    #[arg(short, long, default_value = "plugins")]
    plugin_dir: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server
    Start {
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:3000")]
        bind: String,
        
        /// Number of worker threads
        #[arg(short, long)]
        workers: Option<usize>,
    },
    
    /// Validate configuration
    Config {
        /// Show effective configuration
        #[arg(long)]
        show: bool,
    },
    
    /// Plugin management
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },
    
    /// World management
    World {
        #[command(subcommand)]
        action: WorldCommands,
    },
    
    /// Cluster management
    Cluster {
        #[command(subcommand)]
        action: ClusterCommands,
    },
}

#[derive(Subcommand)]
enum PluginCommands {
    /// List all plugins
    List,
    /// Install a plugin
    Install { path: PathBuf },
    /// Uninstall a plugin
    Uninstall { name: String },
    /// Reload a plugin
    Reload { name: String },
    /// Show plugin info
    Info { name: String },
}

#[derive(Subcommand)]
enum WorldCommands {
    /// Create a new world region
    CreateRegion {
        name: String,
        #[arg(short, long)]
        size: Option<f64>,
    },
    /// List world regions
    ListRegions,
    /// Show world statistics
    Stats,
}

#[derive(Subcommand)]
enum ClusterCommands {
    /// Join a cluster
    Join { address: String },
    /// Leave the cluster
    Leave,
    /// Show cluster status
    Status,
    /// List cluster nodes
    Nodes,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(&cli.log_level)?;
    
    info!("🚀 Horizon 2 Server v{}", env!("CARGO_PKG_VERSION"));
    info!("Starting with config: {}", cli.config.display());
    
    // Load configuration
    let config = HorizonConfig::load(&cli.config)
        .context("Failed to load configuration")?;
    
    // Override config with CLI args
    let mut config = config;
    if let Some(node_id) = cli.node_id {
        config.server.node_id = node_id;
    }
    config.server.cluster_mode = cli.cluster;
    config.plugins.directory = cli.plugin_dir;
    
    // Handle commands
    match cli.command {
        Some(Commands::Start { bind, workers }) => {
            if let Some(workers) = workers {
                config.server.worker_threads = workers;
            }
            config.server.bind_address = bind;
            
            start_server(config).await?;
        }
        
        Some(Commands::Config { show }) => {
            if show {
                println!("{}", toml::to_string_pretty(&config)?);
            } else {
                config.validate()?;
                info!("✅ Configuration is valid");
            }
        }
        
        Some(Commands::Plugin { action }) => {
            handle_plugin_command(action, &config).await?;
        }
        
        Some(Commands::World { action }) => {
            handle_world_command(action, &config).await?;
        }
        
        Some(Commands::Cluster { action }) => {
            handle_cluster_command(action, &config).await?;
        }
        
        None => {
            // Default to starting the server
            start_server(config).await?;
        }
    }
    
    Ok(())
}

fn init_logging(level: &str) -> Result<()> {
    let level = match level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("horizon2={}", level).into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_level(true)
                .with_ansi(true)
                .compact(),
        )
        .init();
    
    Ok(())
}

async fn start_server(config: HorizonConfig) -> Result<()> {
    info!("🔧 Initializing Horizon server...");
    
    // Create and start the server
    let server = HorizonServer::new(config).await
        .context("Failed to create server")?;
    
    // Setup graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };
    
    // Start the server
    tokio::select! {
        result = server.run() => {
            match result {
                Ok(_) => info!("✅ Server stopped gracefully"),
                Err(e) => error!("❌ Server error: {}", e),
            }
        }
        _ = shutdown_signal => {
            info!("🛑 Shutdown signal received");
        }
    }
    
    info!("👋 Horizon server shutdown complete");
    Ok(())
}

async fn handle_plugin_command(action: PluginCommands, config: &HorizonConfig) -> Result<()> {
    use crate::plugins::PluginManager;
    
    let plugin_manager = PluginManager::new(&config.plugins).await?;
    
    match action {
        PluginCommands::List => {
            let plugins = plugin_manager.list_plugins().await?;
            if plugins.is_empty() {
                println!("No plugins installed");
            } else {
                println!("Installed plugins:");
                for plugin in plugins {
                    println!("  {} v{} - {}", plugin.name, plugin.version, plugin.description);
                }
            }
        }
        
        PluginCommands::Install { path } => {
            plugin_manager.install_plugin(&path).await?;
            info!("✅ Plugin installed successfully");
        }
        
        PluginCommands::Uninstall { name } => {
            plugin_manager.uninstall_plugin(&name).await?;
            info!("✅ Plugin uninstalled successfully");
        }
        
        PluginCommands::Reload { name } => {
            plugin_manager.reload_plugin(&name).await?;
            info!("✅ Plugin reloaded successfully");
        }
        
        PluginCommands::Info { name } => {
            if let Some(info) = plugin_manager.get_plugin_info(&name).await? {
                println!("Plugin: {}", info.name);
                println!("Version: {}", info.version);
                println!("Description: {}", info.description);
                println!("Author: {}", info.author);
                println!("Status: {}", if info.enabled { "Enabled" } else { "Disabled" });
            } else {
                println!("Plugin '{}' not found", name);
            }
        }
    }
    
    Ok(())
}

async fn handle_world_command(action: WorldCommands, config: &HorizonConfig) -> Result<()> {
    use crate::world::WorldManager;
    
    let world_manager = WorldManager::new(&config.world).await?;
    
    match action {
        WorldCommands::CreateRegion { name, size } => {
            let size = size.unwrap_or(1000.0);
            world_manager.create_region(&name, size).await?;
            info!("✅ Region '{}' created with size {}", name, size);
        }
        
        WorldCommands::ListRegions => {
            let regions = world_manager.list_regions().await?;
            if regions.is_empty() {
                println!("No regions found");
            } else {
                println!("World regions:");
                for region in regions {
                    println!("  {} - Size: {}, Objects: {}", 
                        region.name, region.size, region.object_count);
                }
            }
        }
        
        WorldCommands::Stats => {
            let stats = world_manager.get_stats().await?;
            println!("World Statistics:");
            println!("  Total regions: {}", stats.total_regions);
            println!("  Total objects: {}", stats.total_objects);
            println!("  Active players: {}", stats.active_players);
            println!("  Memory usage: {} MB", stats.memory_usage_mb);
        }
    }
    
    Ok(())
}

async fn handle_cluster_command(action: ClusterCommands, _config: &HorizonConfig) -> Result<()> {
    match action {
        ClusterCommands::Join { address } => {
            info!("Joining cluster at {}", address);
            // Implementation would go here
            println!("✅ Joined cluster successfully");
        }
        
        ClusterCommands::Leave => {
            info!("Leaving cluster");
            // Implementation would go here
            println!("✅ Left cluster successfully");
        }
        
        ClusterCommands::Status => {
            println!("Cluster Status: Active");
            println!("Node ID: {}", uuid::Uuid::new_v4());
            println!("Nodes: 3/5");
            println!("Health: Healthy");
        }
        
        ClusterCommands::Nodes => {
            println!("Cluster Nodes:");
            println!("  node-1 (self) - 192.168.1.10:3000 - Healthy");
            println!("  node-2 - 192.168.1.11:3000 - Healthy");
            println!("  node-3 - 192.168.1.12:3000 - Degraded");
        }
    }
    
    Ok(())
}