//! Connection management for client connections.
//!
//! This module handles the lifecycle of client connections, including
//! connection tracking, player ID assignment, and message routing.

pub mod client;
pub mod manager;

pub use manager::ConnectionManager;

/// Type alias for connection identifiers.
/// 
/// Connection IDs are used to uniquely identify client connections
/// throughout their lifecycle on the server.
pub type ConnectionId = usize;