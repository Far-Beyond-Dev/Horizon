use anyhow::Result;
use clap::Parser;
use server_core::{GameServer, ServerConfig};
use shared_types::{RegionBounds, ServerError};
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
    
    /// Server listen address
    #[arg(short, long)]
    listen: Option<String>,
    
    /// Plugin directory
    #[arg(short, long)]
    plugins: Option<PathBuf>,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
    
    /// Maximum number of players
    #[arg(long)]
    max_players: Option<usize>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct Config {
    server: ServerSettings,
    region: RegionSettings,
    plugins: PluginSettings,
    logging: Option<LoggingSettings>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct ServerSettings {
    listen_addr: String,
    max_players: usize,
    tick_rate: u64,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct RegionSettings {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
    min_z: f64,
    max_z: f64,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct PluginSettings {
    directory: String,
    auto_load: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
struct LoggingSettings {
    level: String,
    json_format: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerSettings {
                listen_addr: "127.0.0.1:8080".to_string(),
                max_players: 1000,
                tick_rate: 50,
            },
            region: RegionSettings {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -100.0,
                max_z: 100.0,
            },
            plugins: PluginSettings {
                directory: "plugins".to_string(),
                auto_load: vec!["horizon".to_string()],
            },
            logging: Some(LoggingSettings {
                level: "info".to_string(),
                json_format: false,
            }),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();
    
    // Initialize logging
    if let Err(e) = setup_logging(&args) {
        error!("Failed to initialize logging: {}", e);
        return Err(anyhow::anyhow!("Failed to initialize logging: {}", e));
    }
    
    info!("Starting Distributed Games Server");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration
    let config = load_config(&args).await
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    info!("Configuration loaded from: {}", args.config.display());
    
    // Create server configuration
    let region_bounds = RegionBounds {
        min_x: config.region.min_x,
        max_x: config.region.max_x,
        min_y: config.region.min_y,
        max_y: config.region.max_y,
        min_z: config.region.min_z,
        max_z: config.region.max_z,
    };
    
    let server_config = ServerConfig {
        listen_addr: args.listen
            .as_deref()
            .unwrap_or(&config.server.listen_addr)
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse listen address: {}", e))?,
        region_bounds,
        plugin_directory: args.plugins
            .as_deref()
            .unwrap_or(Path::new(&config.plugins.directory))
            .to_string_lossy()
            .to_string(),
        max_players: args.max_players.unwrap_or(config.server.max_players),
        tick_rate: config.server.tick_rate,
    };
    
    // Create and configure the game server
    let mut server = GameServer::new(server_config.region_bounds.clone());
    
    // START EVENT PROCESSOR BEFORE LOADING PLUGINS
    info!("Starting event processor...");
    server.start_event_processor().await;
    
    // Load plugins AFTER starting event processor
    load_plugins(&mut server, &config.plugins, &server_config.plugin_directory)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load plugins: {}", e))?;
    
    // Setup shutdown handler
    let shutdown_server = setup_shutdown_handler().await;
    
    // Start the server (this will NOT start event processor again since it's already running)
    info!("Server configuration:");
    info!("  Listen address: {}", server_config.listen_addr);
    info!("  Region bounds: {:?}", server_config.region_bounds);
    info!("  Max players: {}", server_config.max_players);
    info!("  Tick rate: {}ms", server_config.tick_rate);
    info!("  Plugin directory: {}", server_config.plugin_directory);
    
    // Run server with graceful shutdown
    tokio::select! {
        result = server.start(server_config.listen_addr) => {
            match result {
                Ok(_) => info!("Server stopped normally"),
                Err(e) => {
                    error!("Server error: {}", e);
                    return Err(e.into());
                }
            }
        }
        _ = shutdown_server => {
            info!("Shutdown signal received");
            if let Err(e) = server.shutdown().await {
                error!("Error during shutdown: {}", e);
            }
        }
    }
    
    info!("Server shutdown complete");
    Ok(())
}

fn setup_logging(args: &Args) -> Result<()> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    
    let level = if args.debug { "debug" } else { "info" };
    
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
    
    Ok(())
}

async fn load_config(args: &Args) -> Result<Config> {
    if args.config.exists() {
        let config_str = tokio::fs::read_to_string(&args.config).await?;
        let config: Config = toml::from_str(&config_str)?;
        Ok(config)
    } else {
        warn!("Configuration file not found: {}, using defaults", args.config.display());
        
        // Create default config file
        let default_config = Config::default();
        let config_str = toml::to_string_pretty(&default_config)?;
        tokio::fs::write(&args.config, config_str).await?;
        info!("Created default configuration file: {}", args.config.display());
        
        Ok(default_config)
    }
}

async fn load_plugins(
    server: &mut GameServer,
    plugin_config: &PluginSettings,
    plugin_dir: &str,
) -> Result<(), ServerError> {
    let plugin_path = Path::new(plugin_dir);

    if !plugin_path.exists() {
        warn!("Plugin directory does not exist: {}", plugin_dir);
        if let Err(e) = tokio::fs::create_dir_all(plugin_path).await {
            error!("Failed to create plugin directory: {}", e);
            return Err(ServerError::Internal(format!("Failed to create plugin directory: {}", e)));
        }
        info!("Created plugin directory: {}", plugin_dir);
    }

    // Load auto-load plugins
    for plugin_name in &plugin_config.auto_load {
        let plugin_file = if cfg!(target_os = "windows") {
            format!("{}.dll", plugin_name)
        } else if cfg!(target_os = "macos") {
            format!("lib{}.dylib", plugin_name)
        } else {
            format!("lib{}.so", plugin_name)
        };

        let plugin_path = plugin_path.join(&plugin_file);

        if plugin_path.exists() {
            info!("Loading plugin: {}", plugin_name);
            match server.load_plugin(&plugin_path).await {
                Ok(_) => info!("Successfully loaded plugin: {}", plugin_name),
                Err(e) => {
                    error!("Failed to load plugin {}: {}", plugin_name, e);
                    return Err(e.into());
                }
            }
        } else {
            warn!("Plugin file not found: {}", plugin_path.display());
            info!("Expected plugin file: {}", plugin_file);
            info!("To build the horizon plugin, run: cargo build --release --package horizon-plugin");
        }
    }

    Ok(())
}

async fn setup_shutdown_handler() -> tokio::sync::oneshot::Receiver<()> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let mut tx = Some(tx);
        
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
            
            tokio::select! {
                _ = sigint.recv() => {
                    info!("SIGINT received");
                }
                _ = sigterm.recv() => {
                    info!("SIGTERM received");
                }
            }
        }
        
        #[cfg(windows)]
        {
            use tokio::signal::windows::ctrl_c;
            
            let mut ctrl_c = ctrl_c().expect("Failed to create Ctrl+C handler");
            ctrl_c.recv().await;
            info!("Ctrl+C received");
        }
        
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    });
    
    rx
}

/// Health check endpoint handler (could be extended for HTTP health checks)
async fn health_check() -> Result<String, ServerError> {
    Ok("OK".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_server_startup_shutdown() {
        let region_bounds = RegionBounds {
            min_x: -100.0,
            max_x: 100.0,
            min_y: -100.0,
            max_y: 100.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        
        let server = GameServer::new(region_bounds);
        
        // Test that server can be created and shut down
        let shutdown_result = timeout(Duration::from_millis(100), server.shutdown()).await;
        assert!(shutdown_result.is_ok());
    }
    
    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.server.listen_addr, "127.0.0.1:8080");
        assert_eq!(config.server.max_players, 1000);
        assert!(!config.plugins.auto_load.is_empty());
    }
    
    #[test]
    fn test_region_bounds() {
        let bounds = RegionBounds {
            min_x: -50.0,
            max_x: 50.0,
            min_y: -50.0,
            max_y: 50.0,
            min_z: -5.0,
            max_z: 5.0,
        };
        
        let inside_pos = shared_types::Position::new(0.0, 0.0, 0.0);
        let outside_pos = shared_types::Position::new(100.0, 0.0, 0.0);
        
        assert!(bounds.contains(&inside_pos));
        assert!(!bounds.contains(&outside_pos));
    }
}