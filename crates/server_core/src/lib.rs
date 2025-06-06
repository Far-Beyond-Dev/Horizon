//! Game Server Library
//! 
//! A modular WebSocket-based game server with plugin support for distributed gaming systems.
//! Supports dynamic plugin loading, event-driven architecture, and regional player management.

pub mod server;
pub mod plugin;
pub mod context;

// Re-export main types for convenience
pub use server::{GameServer, ServerConfig};
pub use plugin::PluginLoader;
pub use context::ServerContextImpl;

// Re-export shared types
pub use shared_types::*;