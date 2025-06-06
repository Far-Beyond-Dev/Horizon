//! Distributed Games Server - Main Entry Point
//! 
//! A high-performance, plugin-extensible game server with configurable regions
//! and graceful shutdown handling.

use anyhow::Result;
use clap::Parser;
use server_core::{GameServer, ServerConfig};
use shared_types::RegionBounds;
use std::path::Path;
use tracing::{error, info};

// Import our modular components
use horizon_server::{
    config::{self, Args, Config},
    logging,
    plugins,
    shutdown,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Parse command-line arguments
    let args = Args::parse();
    
    // Initialize logging system
    if let Err(e) = logging::setup_logging(&args) {
        error!("Failed to initialize logging: {}", e);
        return Err(anyhow::anyhow!("Failed to initialize logging: {}", e));
    }
    
    // Log startup information
    info!("Starting Distributed Games Server");
    info!("Version: {}", env!("CARGO_PKG_VERSION"));
    
    // Load configuration from file or create default
    let config = config::load_config(&args).await
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    info!("Configuration loaded from: {}", args.config.display());
    
    // Create server configuration from loaded config and CLI overrides
    let server_config = create_server_config(&config, &args)?;
    
    // Initialize the game server
    let mut server = GameServer::new(server_config.region_bounds.clone());
    
    // Start event processor before loading plugins
    info!("Starting event processor...");
    server.event_processor.start().await;
    
    // Load plugins after starting event processor
    plugins::load_plugins(&mut server, &config.plugins, &server_config.plugin_directory)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load plugins: {}", e))?;
    
    // Set up graceful shutdown handling
    let shutdown_receiver = shutdown::setup_shutdown_handler().await;
    
    // Log final server configuration
    log_server_configuration(&server_config);
    
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
        _ = shutdown_receiver => {
            info!("Shutdown signal received");
            if let Err(e) = server.shutdown().await {
                error!("Error during shutdown: {}", e);
            }
        }
    }
    
    info!("Server shutdown complete");
    Ok(())
}

/// Create server configuration from loaded config and CLI arguments
/// 
/// This function merges configuration file settings with command-line
/// argument overrides to create the final server configuration.
/// 
/// # Arguments
/// * `config` - Loaded configuration from file
/// * `args` - Parsed command-line arguments
/// 
/// # Returns
/// * `Result<ServerConfig>` - Final server configuration or error
fn create_server_config(config: &Config, args: &Args) -> Result<ServerConfig> {
    // Create region bounds from configuration
    let region_bounds = RegionBounds {
        min_x: config.region.min_x,
        max_x: config.region.max_x,
        min_y: config.region.min_y,
        max_y: config.region.max_y,
        min_z: config.region.min_z,
        max_z: config.region.max_z,
    };
    
    // Parse listen address (CLI override takes precedence)
    let listen_addr = args.listen
        .as_deref()
        .unwrap_or(&config.server.listen_addr)
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse listen address: {}", e))?;
    
    // Determine plugin directory (CLI override takes precedence)
    let plugin_directory = args.plugins
        .as_deref()
        .unwrap_or(Path::new(&config.plugins.directory))
        .to_string_lossy()
        .to_string();
    
    // Determine max players (CLI override takes precedence)
    let max_players = args.max_players.unwrap_or(config.server.max_players);
    
    Ok(ServerConfig {
        listen_addr,
        region_bounds,
        plugin_directory,
        max_players,
        tick_rate: config.server.tick_rate,
        ping_interval: config.server.ping_interval,
        connection_timeout: config.server.connection_timeout,
        event_queue_capacity: config.server.event_queue_capacity,
    })
}

/// Log the final server configuration
/// 
/// Outputs all relevant server settings for debugging and verification.
/// 
/// # Arguments
/// * `config` - The final server configuration to log
fn log_server_configuration(config: &ServerConfig) {
    info!("Server configuration:");
    info!("  Listen address: {}", config.listen_addr);
    info!("  Region bounds: {:?}", config.region_bounds);
    info!("  Max players: {}", config.max_players);
    info!("  Tick rate: {}ms", config.tick_rate);
    info!("  Plugin directory: {}", config.plugin_directory);
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{Position, RegionBounds};
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
    fn test_create_server_config() {
        let config = Config::default();
        let args = Args::default();
        
        let server_config = create_server_config(&config, &args).unwrap();
        assert_eq!(server_config.max_players, 1000);
        assert_eq!(server_config.tick_rate, 50);
    }
    
    #[test]
    fn test_create_server_config_with_overrides() {
        let config = Config::default();
        let mut args = Args::default();
        args.max_players = Some(500);
        args.listen = Some("0.0.0.0:9090".to_string());
        
        let server_config = create_server_config(&config, &args).unwrap();
        assert_eq!(server_config.max_players, 500);
        // Note: listen_addr comparison would require parsing SocketAddr
    }
    
    #[test]
    fn test_region_bounds_functionality() {
        let bounds = RegionBounds {
            min_x: -50.0,
            max_x: 50.0,
            min_y: -50.0,
            max_y: 50.0,
            min_z: -5.0,
            max_z: 5.0,
        };
        
        let inside_pos = Position::new(0.0, 0.0, 0.0);
        let outside_pos = Position::new(100.0, 0.0, 0.0);
        
        assert!(bounds.contains(&inside_pos));
        assert!(!bounds.contains(&outside_pos));
    }
}