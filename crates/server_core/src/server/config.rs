//! Server configuration management
//! 
//! Defines configuration structures and defaults for the game server.

use shared_types::RegionBounds;
use std::net::SocketAddr;

/// Configuration settings for the game server
/// 
/// Contains all configurable parameters for server operation including
/// network settings, regional boundaries, plugin configuration, and performance tuning.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Network address and port to bind the server to
    pub listen_addr: SocketAddr,
    
    /// Spatial boundaries for this server region
    /// Players cannot move outside these bounds
    pub region_bounds: RegionBounds,
    
    /// Directory path where plugin libraries are stored
    pub plugin_directory: String,
    
    /// Maximum number of concurrent players allowed
    pub max_players: usize,
    
    /// Server tick rate in milliseconds
    /// Controls how frequently the server processes game state updates
    pub tick_rate: u64,
    
    /// WebSocket ping interval in seconds
    /// How often to send ping messages to detect disconnected clients
    pub ping_interval: u64,
    
    /// Connection timeout in seconds
    /// How long to wait for unresponsive clients before closing connection
    pub connection_timeout: u64,
    
    /// Event queue capacity
    /// Maximum number of events that can be queued before blocking
    pub event_queue_capacity: usize,
}

impl ServerConfig {
    /// Create a new server configuration with custom values
    /// 
    /// # Arguments
    /// * `listen_addr` - Address to bind the server to
    /// * `region_bounds` - Spatial boundaries for the region
    /// 
    /// # Returns
    /// A new ServerConfig with specified values and other defaults
    pub fn new(listen_addr: SocketAddr, region_bounds: RegionBounds) -> Self {
        Self {
            listen_addr,
            region_bounds,
            ..Default::default()
        }
    }
    
    /// Set the plugin directory path
    /// 
    /// # Arguments
    /// * `path` - Directory path for plugin libraries
    pub fn with_plugin_directory(mut self, path: impl Into<String>) -> Self {
        self.plugin_directory = path.into();
        self
    }
    
    /// Set the maximum number of players
    /// 
    /// # Arguments
    /// * `max` - Maximum concurrent players
    pub fn with_max_players(mut self, max: usize) -> Self {
        self.max_players = max;
        self
    }
    
    /// Set the server tick rate
    /// 
    /// # Arguments
    /// * `rate_ms` - Tick rate in milliseconds
    pub fn with_tick_rate(mut self, rate_ms: u64) -> Self {
        self.tick_rate = rate_ms;
        self
    }
    
    /// Validate the configuration
    /// 
    /// # Returns
    /// Result indicating whether the configuration is valid
    /// 
    /// # Errors
    /// Returns error messages for invalid configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.max_players == 0 {
            return Err("max_players must be greater than 0".to_string());
        }
        
        if self.tick_rate == 0 {
            return Err("tick_rate must be greater than 0".to_string());
        }
        
        if self.ping_interval == 0 {
            return Err("ping_interval must be greater than 0".to_string());
        }
        
        if self.connection_timeout <= self.ping_interval {
            return Err("connection_timeout must be greater than ping_interval".to_string());
        }
        
        if self.event_queue_capacity == 0 {
            return Err("event_queue_capacity must be greater than 0".to_string());
        }
        
        // Validate region bounds
        if self.region_bounds.min_x >= self.region_bounds.max_x ||
           self.region_bounds.min_y >= self.region_bounds.max_y ||
           self.region_bounds.min_z >= self.region_bounds.max_z {
            return Err("Invalid region bounds: min values must be less than max values".to_string());
        }
        
        Ok(())
    }
}

impl Default for ServerConfig {
    /// Create a server configuration with sensible defaults
    /// 
    /// Default values:
    /// - Listen address: 127.0.0.1:8080
    /// - Region bounds: 2000x2000x200 units centered at origin
    /// - Plugin directory: "plugins"
    /// - Max players: 1000
    /// - Tick rate: 50ms (20 FPS)
    /// - Ping interval: 30 seconds
    /// - Connection timeout: 60 seconds
    /// - Event queue capacity: 1000 events
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8080".parse().unwrap(),
            region_bounds: RegionBounds {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -100.0,
                max_z: 100.0,
            },
            plugin_directory: "plugins".to_string(),
            max_players: 1000,
            tick_rate: 50, // 20 FPS
            ping_interval: 30,
            connection_timeout: 60,
            event_queue_capacity: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config_is_valid() {
        let config = ServerConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_max_players() {
        let mut config = ServerConfig::default();
        config.max_players = 0;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_invalid_region_bounds() {
        let mut config = ServerConfig::default();
        config.region_bounds.min_x = 100.0;
        config.region_bounds.max_x = 50.0; // Invalid: min > max
        assert!(config.validate().is_err());
    }
}