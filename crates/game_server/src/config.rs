//! Server configuration types and defaults.
//!
//! This module contains the server configuration structure and default values
//! used to initialize and customize the game server behavior.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Simple region bounds for server spatial configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub min_z: f64,
    pub max_z: f64,
}

impl Default for RegionBounds {
    fn default() -> Self {
        Self {
            min_x: -1000.0,
            max_x: 1000.0,
            min_y: -1000.0,
            max_y: 1000.0,
            min_z: -100.0,
            max_z: 100.0,
        }
    }
}

/// Simple plugin safety configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSafetyConfig {
    pub enabled: bool,
    pub sandbox_mode: bool,
}

impl Default for PluginSafetyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sandbox_mode: false,
        }
    }
}

/// Security configuration for input validation and protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Enable rate limiting
    pub enable_rate_limiting: bool,
    
    /// Maximum requests per minute per IP
    pub max_requests_per_minute: u32,
    
    /// Maximum message size in bytes
    pub max_message_size: usize,
    
    /// Maximum allowed nesting depth for JSON messages
    pub max_json_depth: usize,
    
    /// Maximum allowed string length in JSON
    pub max_string_length: usize,
    
    /// Maximum allowed array/object size
    pub max_collection_size: usize,
    
    /// Enable DDoS protection
    pub enable_ddos_protection: bool,
    
    /// Banned IP addresses
    pub banned_ips: Vec<IpAddr>,
    
    /// Maximum concurrent connections per IP
    pub max_connections_per_ip: u32,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: true,
            max_requests_per_minute: 60,
            max_message_size: 64 * 1024, // 64KB
            max_json_depth: 10,
            max_string_length: 1024,
            max_collection_size: 100,
            enable_ddos_protection: true,
            banned_ips: Vec::new(),
            max_connections_per_ip: 10,
        }
    }
}

/// Configuration structure for the game server.
/// 
/// Contains all necessary parameters to configure server behavior including
/// network settings, region boundaries, plugin management, and connection limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The socket address to bind the server to
    pub bind_address: SocketAddr,
    
    /// The spatial bounds for this server region
    pub region_bounds: RegionBounds,
    
    /// Directory path where plugins are stored
    pub plugin_directory: PathBuf,
    
    /// Maximum number of concurrent connections allowed
    pub max_connections: usize,
    
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    
    /// Whether to use SO_REUSEPORT for multi-threaded accept loops
    pub use_reuse_port: bool,
    
    /// Server tick interval in milliseconds (0 to disable)
    pub tick_interval_ms: u64,
    
    /// Security configuration settings
    pub security: SecurityConfig,
    
    /// Plugin safety configuration settings
    pub plugin_safety: PluginSafetyConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().expect("Invalid default bind address"),
            region_bounds: RegionBounds::default(),
            plugin_directory: PathBuf::from("plugins"),
            max_connections: 1000,
            connection_timeout: 60,
            use_reuse_port: false,
            tick_interval_ms: 50, // 20 ticks per second by default
            security: SecurityConfig::default(),
            plugin_safety: PluginSafetyConfig::default(),
        }
    }
}