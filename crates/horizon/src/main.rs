//! Main application entry point for the game server
//! 
//! Provides CLI interface, configuration loading, and server startup.

use clap::{Arg, Command};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Import our modules - note these are direct crate imports since we're in the horizon crate
use types;
use event_system;
use plugin_system;

use game_server::{GameServer, ServerConfig};
use types::*;

// ============================================================================
// Configuration
// ============================================================================

/// Application configuration loaded from TOML file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    pub server: ServerSettings,
    /// Plugin configuration
    pub plugins: PluginSettings,
    /// Logging configuration
    pub logging: LoggingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Bind address
    pub bind_address: String,
    /// Region bounds
    pub region: RegionSettings,
    /// Connection limits
    pub max_connections: usize,
    /// Network timeouts
    pub ping_interval: u64,
    pub connection_timeout: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSettings {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Plugin directory
    pub directory: String,
    /// Auto-load plugins on startup
    pub auto_load: bool,
    /// Plugin whitelist (empty means load all)
    pub whitelist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    /// Log level filter
    pub level: String,
    /// JSON formatting
    pub json_format: bool,
    /// Log to file
    pub file_path: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerSettings {
                bind_address: "127.0.0.1:8080".to_string(),
                region: RegionSettings {
                    min_x: -1000.0,
                    max_x: 1000.0,
                    min_y: -1000.0,
                    max_y: 1000.0,
                    min_z: -100.0,
                    max_z: 100.0,
                },
                max_connections: 1000,
                ping_interval: 30,
                connection_timeout: 60,
            },
            plugins: PluginSettings {
                directory: "plugins".to_string(),
                auto_load: true,
                whitelist: vec![],
            },
            logging: LoggingSettings {
                level: "info".to_string(),
                json_format: false,
                file_path: None,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from file
    pub async fn load_from_file(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        if path.exists() {
            let content = tokio::fs::read_to_string(path).await?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            // Create default config file
            let default_config = AppConfig::default();
            let toml_content = toml::to_string_pretty(&default_config)?;
            tokio::fs::write(path, toml_content).await?;
            info!("Created default configuration file: {}", path.display());
            Ok(default_config)
        }
    }
    
    /// Convert to ServerConfig
    pub fn to_server_config(&self) -> Result<ServerConfig, Box<dyn std::error::Error>> {
        Ok(ServerConfig {
            bind_address: self.server.bind_address.parse()?,
            region_bounds: RegionBounds {
                min_x: self.server.region.min_x,
                max_x: self.server.region.max_x,
                min_y: self.server.region.min_y,
                max_y: self.server.region.max_y,
                min_z: self.server.region.min_z,
                max_z: self.server.region.max_z,
            },
            plugin_directory: PathBuf::from(&self.plugins.directory),
            max_connections: self.server.max_connections,
            ping_interval: self.server.ping_interval,
            connection_timeout: self.server.connection_timeout,
        })
    }
}

// ============================================================================
// CLI Interface
// ============================================================================

/// Command line arguments
#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: PathBuf,
    pub plugin_dir: Option<PathBuf>,
    pub bind_address: Option<String>,
    pub log_level: Option<String>,
    pub json_logs: bool,
}

impl CliArgs {
    /// Parse command line arguments
    pub fn parse() -> Self {
        let matches = Command::new("Game Server")
            .version("1.0.0")
            .author("Your Name <your.email@example.com>")
            .about("High-performance game server with plugin system")
            .arg(
                Arg::new("config")
                    .short('c')
                    .long("config")
                    .value_name("FILE")
                    .help("Configuration file path")
                    .default_value("config.toml")
            )
            .arg(
                Arg::new("plugins")
                    .short('p')
                    .long("plugins")
                    .value_name("DIR")
                    .help("Plugin directory path")
            )
            .arg(
                Arg::new("bind")
                    .short('b')
                    .long("bind")
                    .value_name("ADDRESS")
                    .help("Bind address (e.g., 127.0.0.1:8080)")
            )
            .arg(
                Arg::new("log-level")
                    .short('l')
                    .long("log-level")
                    .value_name("LEVEL")
                    .help("Log level (trace, debug, info, warn, error)")
            )
            .arg(
                Arg::new("json-logs")
                    .long("json-logs")
                    .help("Output logs in JSON format")
                    .action(clap::ArgAction::SetTrue)
            )
            .get_matches();
        
        Self {
            config_path: PathBuf::from(matches.get_one::<String>("config").unwrap()),
            plugin_dir: matches.get_one::<String>("plugins").map(PathBuf::from),
            bind_address: matches.get_one::<String>("bind").cloned(),
            log_level: matches.get_one::<String>("log-level").cloned(),
            json_logs: matches.get_flag("json-logs"),
        }
    }
}

// ============================================================================
// Logging Setup
// ============================================================================

/// Initialize logging system
fn setup_logging(config: &LoggingSettings, json_format: bool) -> Result<(), Box<dyn std::error::Error>> {
    let log_level = config.level.as_str();
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));
    
    let registry = tracing_subscriber::registry().with(filter);
    
    if json_format || config.json_format {
        // JSON formatting
        registry.with(fmt::layer().json()).init();
    } else {
        // Human-readable formatting
        registry.with(fmt::layer().pretty()).init();
    }
    
    info!("Logging initialized with level: {}", log_level);
    Ok(())
}

// ============================================================================
// Signal Handling
// ============================================================================

/// Setup graceful shutdown signal handling
async fn setup_signal_handlers() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use signal::unix::{signal, SignalKind};
        
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        
        tokio::select! {
            _ = sigint.recv() => {
                info!("Received SIGINT");
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM");
            }
        }
    }
    
    #[cfg(windows)]
    {
        signal::ctrl_c().await?;
        info!("Received Ctrl+C");
    }
    
    Ok(())
}

// ============================================================================
// Main Application
// ============================================================================

/// Main application struct
pub struct Application {
    config: AppConfig,
    server: GameServer,
}

impl Application {
    /// Create new application
    pub async fn new(args: CliArgs) -> Result<Self, Box<dyn std::error::Error>> {
        // Load configuration
        let mut config = AppConfig::load_from_file(&args.config_path).await?;
        
        // Apply CLI overrides
        if let Some(plugin_dir) = args.plugin_dir {
            config.plugins.directory = plugin_dir.to_string_lossy().to_string();
        }
        
        if let Some(bind_address) = args.bind_address {
            config.server.bind_address = bind_address;
        }
        
        if let Some(log_level) = args.log_level {
            config.logging.level = log_level;
        }
        
        if args.json_logs {
            config.logging.json_format = true;
        }
        
        // Setup logging
        setup_logging(&config.logging, args.json_logs)?;
        
        // Create server
        let server_config = config.to_server_config()?;
        let server = GameServer::new(server_config);
        
        Ok(Self { config, server })
    }
    
    /// Run the application
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting game server application");
        info!("Configuration loaded from config file");
        info!("Plugin directory: {}", self.config.plugins.directory);
        info!("Bind address: {}", self.config.server.bind_address);
        
        // Start server in background
        let server_handle = {
            let server = self.server;
            tokio::spawn(async move {
                if let Err(e) = server.start().await {
                    error!("Server error: {}", e);
                }
            })
        };
        
        // Wait for shutdown signal
        info!("Server started. Press Ctrl+C to stop.");
        setup_signal_handlers().await?;
        
        info!("Shutting down...");
        
        // Cancel server task
        server_handle.abort();
        
        // Give some time for cleanup
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        info!("Application shut down complete");
        Ok(())
    }
}

// ============================================================================
// Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI arguments
    let args = CliArgs::parse();
    
    // Create and run application
    let app = Application::new(args).await?;
    app.run().await?;
    
    Ok(())

    //drop my_number;
}