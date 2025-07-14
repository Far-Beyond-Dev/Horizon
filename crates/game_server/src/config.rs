//! Server configuration types and defaults.
//!
//! This module contains the server configuration structure and default values
//! used to initialize and customize the game server behavior.

use horizon_event_system::RegionBounds;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Configuration structure for the game server.
/// 
/// Contains all necessary parameters to configure server behavior including
/// network settings, region boundaries, plugin management, and connection limits.
#[derive(Debug, Clone)]
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
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().expect("Attempted to use ServerConfig.default(), but field `bind_address` is not parsable in the source code"),
            region_bounds: RegionBounds {
                min_x: -1000.0,
                max_x: 1000.0,
                min_y: -1000.0,
                max_y: 1000.0,
                min_z: -100.0,
                max_z: 100.0,
            },
            plugin_directory: PathBuf::from("plugins"),
            max_connections: 1000,
            connection_timeout: 60,
            use_reuse_port: false
        }
    }
}