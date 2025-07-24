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
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    /// Whether to use SO_REUSEPORT for multi-threaded accept loops (Linux only)
    #[serde(default)]
    pub use_reuse_port: bool,
    /// Server tick interval in milliseconds (0 to disable)
    #[serde(default = "default_tick_interval")]
    pub tick_interval_ms: u64,
}

/// Default for connection_timeout
pub fn default_connection_timeout() -> u64 {
    60
}

/// Default for max_connections
fn default_max_connections() -> usize {
    1000
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;
    use tokio::fs;

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        
        // Test server settings
        assert_eq!(config.server.bind_address, "127.0.0.1:8080");
        assert_eq!(config.server.max_connections, 1000);
        assert_eq!(config.server.connection_timeout, 60);
        assert_eq!(config.server.use_reuse_port, false);
        assert_eq!(config.server.tick_interval_ms, 50);
        
        // Test region settings
        assert_eq!(config.server.region.min_x, -1000.0);
        assert_eq!(config.server.region.max_x, 1000.0);
        assert_eq!(config.server.region.min_y, -1000.0);
        assert_eq!(config.server.region.max_y, 1000.0);
        assert_eq!(config.server.region.min_z, -100.0);
        assert_eq!(config.server.region.max_z, 100.0);
        
        // Test plugin settings
        assert_eq!(config.plugins.directory, "plugins");
        assert_eq!(config.plugins.auto_load, true);
        assert!(config.plugins.whitelist.is_empty());
        
        // Test logging settings
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.logging.json_format, false);
        assert!(config.logging.file_path.is_none());
    }

    #[test]
    fn test_server_settings_creation() {
        let settings = ServerSettings {
            bind_address: "0.0.0.0:9999".to_string(),
            region: RegionSettings {
                min_x: -2000.0,
                max_x: 2000.0,
                min_y: -1500.0,
                max_y: 1500.0,
                min_z: -200.0,
                max_z: 300.0,
            },
            max_connections: 5000,
            connection_timeout: 120,
            use_reuse_port: true,
            tick_interval_ms: 16,
        };

        assert_eq!(settings.bind_address, "0.0.0.0:9999");
        assert_eq!(settings.max_connections, 5000);
        assert_eq!(settings.connection_timeout, 120);
        assert_eq!(settings.use_reuse_port, true);
        assert_eq!(settings.tick_interval_ms, 16);
        assert_eq!(settings.region.min_x, -2000.0);
        assert_eq!(settings.region.max_x, 2000.0);
    }

    #[test]
    fn test_plugin_settings_creation() {
        let settings = PluginSettings {
            directory: "/custom/plugins".to_string(),
            auto_load: false,
            whitelist: vec!["plugin1".to_string(), "plugin2".to_string()],
        };

        assert_eq!(settings.directory, "/custom/plugins");
        assert_eq!(settings.auto_load, false);
        assert_eq!(settings.whitelist.len(), 2);
        assert!(settings.whitelist.contains(&"plugin1".to_string()));
        assert!(settings.whitelist.contains(&"plugin2".to_string()));
    }

    #[test]
    fn test_logging_settings_creation() {
        let settings = LoggingSettings {
            level: "debug".to_string(),
            json_format: true,
            file_path: Some("/var/log/horizon.log".to_string()),
        };

        assert_eq!(settings.level, "debug");
        assert_eq!(settings.json_format, true);
        assert_eq!(settings.file_path, Some("/var/log/horizon.log".to_string()));
    }

    #[tokio::test]
    async fn test_load_from_nonexistent_file() {
        let temp_path = PathBuf::from("nonexistent_config.toml");
        
        // Ensure file doesn't exist
        if temp_path.exists() {
            fs::remove_file(&temp_path).await.ok();
        }

        let result = AppConfig::load_from_file(&temp_path).await;
        assert!(result.is_ok());
        
        let config = result.unwrap();
        
        // Should return default config
        assert_eq!(config.server.bind_address, "127.0.0.1:8080");
        assert_eq!(config.server.tick_interval_ms, 50);
        
        // Should create the file
        assert!(temp_path.exists());
        
        // Clean up
        fs::remove_file(&temp_path).await.ok();
    }

    #[tokio::test]
    async fn test_load_from_existing_file() {
        let toml_content = r#"
[server]
bind_address = "0.0.0.0:3000"
max_connections = 2000
connection_timeout = 90
use_reuse_port = true
tick_interval_ms = 33

[server.region]
min_x = -500.0
max_x = 500.0
min_y = -400.0
max_y = 400.0
min_z = -50.0
max_z = 150.0

[plugins]
directory = "custom_plugins"
auto_load = false
whitelist = ["essential_plugin"]

[logging]
level = "debug"
json_format = true
file_path = "/tmp/test.log"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        fs::write(temp_file.path(), toml_content).await.unwrap();

        let result = AppConfig::load_from_file(&temp_file.path().to_path_buf()).await;
        assert!(result.is_ok());

        let config = result.unwrap();
        
        // Verify server settings
        assert_eq!(config.server.bind_address, "0.0.0.0:3000");
        assert_eq!(config.server.max_connections, 2000);
        assert_eq!(config.server.connection_timeout, 90);
        assert_eq!(config.server.use_reuse_port, true);
        assert_eq!(config.server.tick_interval_ms, 33);
        
        // Verify region settings
        assert_eq!(config.server.region.min_x, -500.0);
        assert_eq!(config.server.region.max_x, 500.0);
        assert_eq!(config.server.region.min_y, -400.0);
        assert_eq!(config.server.region.max_y, 400.0);
        assert_eq!(config.server.region.min_z, -50.0);
        assert_eq!(config.server.region.max_z, 150.0);
        
        // Verify plugin settings
        assert_eq!(config.plugins.directory, "custom_plugins");
        assert_eq!(config.plugins.auto_load, false);
        assert_eq!(config.plugins.whitelist, vec!["essential_plugin"]);
        
        // Verify logging settings
        assert_eq!(config.logging.level, "debug");
        assert_eq!(config.logging.json_format, true);
        assert_eq!(config.logging.file_path, Some("/tmp/test.log".to_string()));
    }

    #[test]
    fn test_to_server_config_conversion() {
        let app_config = AppConfig {
            server: ServerSettings {
                bind_address: "192.168.1.100:8080".to_string(),
                region: RegionSettings {
                    min_x: -1500.0,
                    max_x: 1500.0,
                    min_y: -1200.0,
                    max_y: 1200.0,
                    min_z: -150.0,
                    max_z: 200.0,
                },
                max_connections: 3000,
                connection_timeout: 180,
                use_reuse_port: true,
                tick_interval_ms: 25,
            },
            plugins: PluginSettings {
                directory: "/srv/plugins".to_string(),
                auto_load: true,
                whitelist: vec![],
            },
            logging: LoggingSettings {
                level: "warn".to_string(),
                json_format: false,
                file_path: None,
            },
        };

        let server_config = app_config.to_server_config().unwrap();
        
        assert_eq!(server_config.bind_address.to_string(), "192.168.1.100:8080");
        assert_eq!(server_config.max_connections, 3000);
        assert_eq!(server_config.connection_timeout, 180);
        assert_eq!(server_config.use_reuse_port, true);
        assert_eq!(server_config.tick_interval_ms, 25);
        assert_eq!(server_config.plugin_directory, PathBuf::from("/srv/plugins"));
        assert_eq!(server_config.region_bounds.min_x, -1500.0);
        assert_eq!(server_config.region_bounds.max_x, 1500.0);
    }

    #[test]
    fn test_validation_valid_config() {
        let config = AppConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_invalid_bind_address() {
        let mut config = AppConfig::default();
        config.server.bind_address = "invalid_address".to_string();
        
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid bind address"));
    }

    #[test]
    fn test_validation_invalid_region_bounds() {
        let mut config = AppConfig::default();
        
        // Test min_x >= max_x
        config.server.region.min_x = 100.0;
        config.server.region.max_x = 50.0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_x must be less than max_x"));
        
        // Fix x bounds, test y bounds
        config.server.region.min_x = -100.0;
        config.server.region.max_x = 100.0;
        config.server.region.min_y = 200.0;
        config.server.region.max_y = 100.0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_y must be less than max_y"));
        
        // Fix y bounds, test z bounds
        config.server.region.min_y = -200.0;
        config.server.region.max_y = 200.0;
        config.server.region.min_z = 50.0;
        config.server.region.max_z = 25.0;
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("min_z must be less than max_z"));
    }

    #[test]
    fn test_validation_empty_plugin_directory() {
        let mut config = AppConfig::default();
        config.plugins.directory = "".to_string();
        
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Plugin directory cannot be empty"));
    }

    #[test]
    fn test_validation_invalid_log_level() {
        let mut config = AppConfig::default();
        config.logging.level = "invalid_level".to_string();
        
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid log level"));
    }

    #[test]
    fn test_validation_valid_log_levels() {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        
        for level in &valid_levels {
            let mut config = AppConfig::default();
            config.logging.level = level.to_string();
            
            let result = config.validate();
            assert!(result.is_ok(), "Level '{}' should be valid", level);
        }
    }

    #[test]
    fn test_default_tick_interval_function() {
        assert_eq!(default_tick_interval(), 50);
    }

    #[test]
    fn test_serde_deserialization_with_defaults() {
        let toml_content = r#"

[server]
bind_address = "127.0.0.1:8080"
max_connections = 1000
connection_timeout = 60

[server.region]
min_x = -1000.0
max_x = 1000.0
min_y = -1000.0
max_y = 1000.0
min_z = -100.0
max_z = 100.0

[plugins]
directory = "plugins"
auto_load = true
whitelist = []

[logging]
level = "info"
json_format = false
"#;

        let config: AppConfig = toml::from_str(toml_content).unwrap();
        
        // Should use default values for missing fields
        assert_eq!(config.server.max_connections, 1000);
        assert_eq!(config.server.connection_timeout, 60);
        assert_eq!(config.server.use_reuse_port, false);
        assert_eq!(config.server.tick_interval_ms, 50); // Default from default_tick_interval()
        assert!(config.logging.file_path.is_none());
    }

    #[test]
    fn test_edge_case_configurations() {
        // Test zero tick interval (disabled)
        let mut config = AppConfig::default();
        config.server.tick_interval_ms = 0;
        assert!(config.validate().is_ok());
        
        // Test very high tick interval
        config.server.tick_interval_ms = 10000;
        assert!(config.validate().is_ok());
        
        // Test single connection
        config.server.max_connections = 1;
        assert!(config.validate().is_ok());
        
        // Test very long timeout
        config.server.connection_timeout = 86400; // 24 hours
        assert!(config.validate().is_ok());
        
        // Test minimal region (single point)
        config.server.region = RegionSettings {
            min_x: 0.0,
            max_x: 0.1,
            min_y: 0.0,
            max_y: 0.1,
            min_z: 0.0,
            max_z: 0.1,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_cloning() {
        let config = AppConfig::default();
        let cloned_config = config.clone();
        
        assert_eq!(config.server.bind_address, cloned_config.server.bind_address);
        assert_eq!(config.server.tick_interval_ms, cloned_config.server.tick_interval_ms);
        assert_eq!(config.plugins.directory, cloned_config.plugins.directory);
        assert_eq!(config.logging.level, cloned_config.logging.level);
    }

    #[test]
    fn test_config_debug_formatting() {
        let config = AppConfig::default();
        let debug_str = format!("{:?}", config);
        
        // Verify debug output contains key fields
        assert!(debug_str.contains("bind_address"));
        assert!(debug_str.contains("tick_interval_ms"));
        assert!(debug_str.contains("plugins"));
        assert!(debug_str.contains("logging"));
    }
}