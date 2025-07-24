//! Configuration management for the Horizon game server.
//!
//! This module handles loading, validation, and conversion of server configuration
//! from TOML files and command-line arguments.

use horizon_event_system::RegionBounds;
use game_server::ServerConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

/// Default tick interval for serde deserialization
fn default_tick_interval() -> u64 {
    50 // 20 ticks per second
}

/// Application configuration loaded from TOML file.
/// 
/// This is the main configuration structure that encompasses all server settings
/// including networking, plugins, logging, and region management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration settings
    pub server: ServerSettings,
    /// Plugin configuration settings
    pub plugins: PluginSettings,
    /// Logging configuration settings
    pub logging: LoggingSettings,
}

/// Server-specific configuration settings.
/// 
/// Controls network binding, connection limits, timeouts, and spatial region boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Network address to bind the server to (e.g., "127.0.0.1:8080")
    pub bind_address: String,
    /// Spatial region boundaries for this server instance
    pub region: RegionSettings,
    /// Maximum number of concurrent client connections
    pub max_connections: usize,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Whether to use SO_REUSEPORT for multi-threaded accept loops (Linux only)
    #[serde(default)]
    pub use_reuse_port: bool,
    /// Server tick interval in milliseconds (0 to disable)
    #[serde(default = "default_tick_interval")]
    pub tick_interval_ms: u64,
}

/// Spatial region boundary configuration.
/// 
/// Defines the 3D coordinate space that this server instance manages.
/// Objects and players outside these boundaries may be handled by other server instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionSettings {
    /// Minimum X coordinate
    pub min_x: f64,
    /// Maximum X coordinate
    pub max_x: f64,
    /// Minimum Y coordinate
    pub min_y: f64,
    /// Maximum Y coordinate
    pub max_y: f64,
    /// Minimum Z coordinate
    pub min_z: f64,
    /// Maximum Z coordinate
    pub max_z: f64,
}

/// Plugin system configuration.
/// 
/// Controls plugin loading behavior, directory locations, and security settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSettings {
    /// Directory path where plugin files are located
    pub directory: String,
    /// Whether to automatically load all plugins on startup
    pub auto_load: bool,
    /// Plugin whitelist - if non-empty, only these plugins will be loaded
    pub whitelist: Vec<String>,
}

/// Logging system configuration.
/// 
/// Controls log output format, levels, and destination settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSettings {
    /// Log level filter (trace, debug, info, warn, error)
    pub level: String,
    /// Whether to output logs in JSON format
    pub json_format: bool,
    /// Optional file path for log output (None means stdout only)
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
                connection_timeout: 60,
                use_reuse_port: false,
                tick_interval_ms: 50,
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
    /// Loads configuration from a TOML file.
    /// 
    /// If the file doesn't exist, creates a default configuration file at the specified path
    /// and returns the default configuration.
    /// 
    /// # Arguments
    /// 
    /// * `path` - Path to the configuration file
    /// 
    /// # Returns
    /// 
    /// The loaded or default configuration, or an error if loading/creation failed.
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

    /// Converts the application configuration to a game server configuration.
    /// 
    /// This method translates the TOML-based configuration into the types
    /// expected by the game server core.
    /// 
    /// # Returns
    /// 
    /// A `ServerConfig` instance ready for use with the game server.
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
            connection_timeout: self.server.connection_timeout,
            use_reuse_port: self.server.use_reuse_port,
            tick_interval_ms: self.server.tick_interval_ms,
        })
    }

    /// Validates the configuration for consistency and correctness.
    /// 
    /// Checks network addresses, region boundaries, plugin settings, and other
    /// configuration values for validity.
    /// 
    /// # Returns
    /// 
    /// `Ok(())` if the configuration is valid, or an error string describing the issue.
    pub fn validate(&self) -> Result<(), String> {
        // Validate bind address
        if self.server.bind_address.parse::<std::net::SocketAddr>().is_err() {
            return Err(format!(
                "Invalid bind address: {}",
                &self.server.bind_address
            ));
        }

        // Validate region bounds
        if self.server.region.min_x >= self.server.region.max_x {
            return Err("Region min_x must be less than max_x".to_string());
        }
        if self.server.region.min_y >= self.server.region.max_y {
            return Err("Region min_y must be less than max_y".to_string());
        }
        if self.server.region.min_z >= self.server.region.max_z {
            return Err("Region min_z must be less than max_z".to_string());
        }

        // Validate plugin directory
        if self.plugins.directory.is_empty() {
            return Err("Plugin directory cannot be empty".to_string());
        }

        // Validate log level
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(format!(
                "Invalid log level: {}. Must be one of: {valid_levels:?}",
                &self.logging.level
            ));
        }

        Ok(())
    }
}