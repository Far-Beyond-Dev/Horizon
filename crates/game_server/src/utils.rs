//! Utility functions and helper methods for the game server.
//!
//! This module provides convenient factory functions and utilities
//! for creating server instances with different configurations.

use crate::{config::ServerConfig, server::GameServer};

/// Creates a new game server with default configuration.
/// 
/// This is a convenience function for quickly setting up a server
/// with sensible defaults for development and testing.
/// 
/// # Returns
/// 
/// A new `GameServer` instance configured with default settings.
/// 
/// # Example
/// 
/// ```rust
/// # #[tokio::main]
/// # async fn main() {
/// use game_server::create_server;
/// 
/// let server = create_server();
/// # }
/// ```
pub fn create_server() -> GameServer {
    GameServer::new(ServerConfig::default())
}

/// Creates a new game server with custom configuration.
/// 
/// This function allows full customization of server behavior
/// through the provided configuration object.
/// 
/// # Arguments
/// 
/// * `config` - A `ServerConfig` instance with desired settings
/// 
/// # Returns
/// 
/// A new `GameServer` instance configured with the provided settings.
/// 
/// # Example
/// 
/// ```rust
/// # #[tokio::main]
/// # async fn main() {
/// use game_server::{create_server_with_config, ServerConfig};
/// use std::net::SocketAddr;
/// 
/// let config = ServerConfig {
///     bind_address: "0.0.0.0:9000".parse().unwrap(),
///     max_connections: 5000,
///     ..Default::default()
/// };
/// 
/// let server = create_server_with_config(config);
/// # }
/// ```
pub fn create_server_with_config(config: ServerConfig) -> GameServer {
    GameServer::new(config)
}