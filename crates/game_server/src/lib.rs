//! # Game Server - Clean Universal Plugin Foundation
//!
//! A production-ready game server focused on providing clean, modular infrastructure
//! for multiplayer game development. This server handles core networking, connection
//! management, and universal plugin orchestration while delegating all game logic to plugins.
//!
//! ## Design Philosophy
//!
//! The game server core contains **NO game logic** - it only provides infrastructure:
//!
//! * **WebSocket connection management** - Handles client connections and message routing
//! * **Universal plugin system** - Dynamic loading and management of game logic
//! * **Event-driven architecture** - Clean separation between infrastructure and game code
//! * **Multi-threaded networking** - Scalable accept loops for high-performance operation
//!
//! All game mechanics, rules, and behaviors are implemented as plugins that communicate
//! through the universal event system.
//!
//! ## Architecture Overview
//!
//! ### Core Components
//!
//! * **Universal Event Bus** - Central hub for all plugin communication
//! * **Connection Manager** - WebSocket lifecycle and player mapping  
//! * **Plugin Manager** - Dynamic loading and management of game logic
//!
//! ### Message Flow
//!
//! 1. Client sends WebSocket message with `{namespace, event, data}` structure
//! 2. Server parses and validates the message format
//! 3. Message is routed to plugins via the universal event system
//! 4. Plugins process the message and emit responses
//! 5. Responses are sent back to clients through the connection manager
//!
//! ### Plugin Integration
//!
//! Plugins register event handlers for specific domain/event combinations using
//! the universal plugin system.
//!
//! ## Configuration
//!
//! The server can be configured through the [`ServerConfig`] struct:
//!
//! * **Network settings** - Bind address, connection limits, timeouts
//! * **Plugin management** - Plugin directory and loading behavior
//! * **Performance tuning** - Multi-threading and resource limits
//!
//! ## Error Handling
//!
//! The server uses structured error types ([`ServerError`]) to categorize failures:
//!
//! * **Network errors** - Connection, binding, and protocol issues
//! * **Internal errors** - Plugin failures and event system problems
//!
//! ## Thread Safety
//!
//! All server components are designed for safe concurrent access:
//!
//! * Connection management uses `Arc<RwLock<HashMap>>` for thread-safe state
//! * Universal event system provides async-safe handler registration and emission
//! * Plugin system coordinates safe loading and unloading of plugins
//!
//! ## Performance Considerations
//!
//! * **Multi-threaded accept loops** - Configure `use_reuse_port` for CPU core scaling
//! * **Efficient message routing** - Zero-copy message passing where possible  
//! * **Plugin isolation** - Plugins run in separate contexts to prevent interference
//! * **Connection pooling** - Reuse connections and minimize allocation overhead

// Re-export core types and functions for easy access
pub use config::ServerConfig;
pub use error::ServerError;  
pub use server::GameServer;
pub use utils::{create_server, create_server_with_config};

// Public module declarations
pub mod config;
pub mod error;
pub mod server;
pub mod utils;
pub mod security;
pub mod health;

// Horizon compatibility layer for existing code
pub mod horizon_compat;

// Internal modules (not part of public API)
mod connection;
mod messaging;
mod tests;