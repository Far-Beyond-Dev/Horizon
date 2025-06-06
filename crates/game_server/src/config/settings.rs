//! Configuration settings structures
//! 
//! This module defines all the configuration structures used by the server,
//! including server settings, region bounds, plugin configuration, and logging options.

use serde::{Deserialize, Serialize};

/// Main configuration structure
/// 
/// This is the root configuration object that contains all server settings.
/// It can be serialized to/from TOML format for configuration files.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    /// Server-specific settings
    pub server: ServerSettings,
    /// Game region boundary settings
    pub region: RegionSettings,
    /// Plugin loading and management settings
    pub plugins: PluginSettings,
    /// Optional logging configuration
    pub logging: Option<LoggingSettings>,
}

/// Server configuration settings
/// 
/// Contains core server parameters like network address, player limits,
/// and game loop timing.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ServerSettings {
    /// Network address to bind the server to
    /// 
    /// Format: "IP:PORT" (e.g., "127.0.0.1:8080" for localhost,
    /// "0.0.0.0:8080" for all interfaces)
    pub listen_addr: String,
    
    /// Maximum number of concurrent players
    /// 
    /// This limit helps control server resource usage and maintains
    /// performance under load.
    pub max_players: usize,
    
    /// Server tick rate in milliseconds
    /// 
    /// Controls how frequently the game loop runs. Lower values mean
    /// higher update frequency but more CPU usage.
    pub tick_rate: u64,
    
    /// How often to ping clients (in milliseconds)
    pub ping_interval: u64,
    
    /// Connection timeout in milliseconds
    pub connection_timeout: u64,
    
    /// Capacity of the event queue
    pub event_queue_capacity: usize,
}

/// Game region boundary settings
/// 
/// Defines the 3D boundaries of the game world. Players and entities
/// outside these bounds may be handled differently by the server.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RegionSettings {
    /// Minimum X coordinate
    pub min_x: f64,
    /// Maximum X coordinate  
    pub max_x: f64,
    /// Minimum Y coordinate
    pub min_y: f64,
    /// Maximum Y coordinate
    pub max_y: f64,
    /// Minimum Z coordinate (height/depth)
    pub min_z: f64,
    /// Maximum Z coordinate (height/depth)
    pub max_z: f64,
}

/// Plugin system configuration
/// 
/// Controls how plugins are loaded and managed by the server.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PluginSettings {
    /// Directory path where plugin files are stored
    /// 
    /// The server will search this directory for dynamic libraries
    /// to load as plugins.
    pub directory: String,
    
    /// List of plugin names to automatically load on startup
    /// 
    /// These plugins will be loaded in the order specified.
    /// Plugin names should not include file extensions.
    pub auto_load: Vec<String>,
}

/// Logging system configuration
/// 
/// Controls how the server outputs log messages and diagnostic information.
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LoggingSettings {
    /// Logging level filter
    /// 
    /// Valid values: "trace", "debug", "info", "warn", "error"
    /// Higher levels include all lower levels.
    pub level: String,
    
    /// Enable JSON-formatted log output
    /// 
    /// When true, logs are output in structured JSON format,
    /// useful for log aggregation systems.
    pub json_format: bool,
}

impl Default for Config {
    /// Create a default configuration suitable for development
    /// 
    /// This provides sensible defaults that work out of the box
    /// for local development and testing.
    fn default() -> Self {
        Self {
            server: ServerSettings {
                listen_addr: "127.0.0.1:8080".to_string(),
                max_players: 1000,
                tick_rate: 50,
                ping_interval: 1000,
                connection_timeout: 10000,
                event_queue_capacity: 1024,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.server.listen_addr, "127.0.0.1:8080");
        assert_eq!(config.server.max_players, 1000);
        assert_eq!(config.server.tick_rate, 50);
        assert_eq!(config.server.ping_interval, 1000);
        assert_eq!(config.server.connection_timeout, 10000);
        assert_eq!(config.server.event_queue_capacity, 1024);
        assert!(!config.plugins.auto_load.is_empty());
        assert!(config.logging.is_some());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();
        
        assert_eq!(config.server.listen_addr, deserialized.server.listen_addr);
        assert_eq!(config.server.max_players, deserialized.server.max_players);
        assert_eq!(config.server.tick_rate, deserialized.server.tick_rate);
        assert_eq!(config.server.ping_interval, deserialized.server.ping_interval);
    }

    #[test]
    fn test_region_bounds() {
        let region = RegionSettings {
            min_x: -50.0,
            max_x: 50.0,
            min_y: -50.0,
            max_y: 50.0,
            min_z: -5.0,
            max_z: 5.0,
        };
        
        // Test that region settings can be created with custom bounds
        assert_eq!(region.min_x, -50.0);
        assert_eq!(region.max_x, 50.0);
    }

    #[test]
    fn test_toml_parsing() {
        let toml_str = r#"
[server]
listen_addr = "127.0.0.1:8080"
max_players = 1000
tick_rate = 50
ping_interval = 1000
connection_timeout = 10000
event_queue_capacity = 1024

[region]
min_x = -1000.0
max_x = 1000.0
min_y = -1000.0
max_y = 1000.0
min_z = -100.0
max_z = 100.0

[plugins]
directory = "plugins"
auto_load = ["horizon"]

[logging]
level = "info"
json_format = false
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server.tick_rate, 50);
        assert_eq!(config.server.ping_interval, 1000);
        assert_eq!(config.server.connection_timeout, 10000);
        assert_eq!(config.server.event_queue_capacity, 1024);
    }
}