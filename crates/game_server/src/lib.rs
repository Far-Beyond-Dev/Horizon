//! # Game Server - Clean Infrastructure Foundation
//!
//! A production-ready game server focused on providing clean, modular infrastructure
//! for multiplayer game development. This server handles core networking, connection
//! management, and plugin orchestration while delegating all game logic to plugins.
//!
//! ## Design Philosophy
//!
//! The game server core contains **NO game logic** - it only provides infrastructure:
//!
//! * **WebSocket connection management** - Handles client connections and message routing
//! * **Plugin system integration** - Dynamic loading and management of game logic
//! * **Event-driven architecture** - Clean separation between infrastructure and game code
//! * **GORC integration** - Advanced replication and spatial management capabilities
//! * **Multi-threaded networking** - Scalable accept loops for high-performance operation
//!
//! All game mechanics, rules, and behaviors are implemented as plugins that communicate
//! through the standardized event system.
//!
//! ## Architecture Overview
//!
//! ### Core Components
//!
//! * **Event System** - Central hub for all plugin communication
//! * **Connection Manager** - WebSocket lifecycle and player mapping  
//! * **Plugin Manager** - Dynamic loading and management of game logic
//! * **GORC Components** - Advanced replication and spatial systems
//!
//! ### Message Flow
//!
//! 1. Client sends WebSocket message with `{namespace, event, data}` structure
//! 2. Server parses and validates the message format
//! 3. Message is routed to plugins via the event system
//! 4. Plugins process the message and emit responses
//! 5. Responses are sent back to clients through the connection manager
//!
//! ### Plugin Integration
//!
//! Plugins register event handlers for specific namespace/event combinations:
//!
//! ```rust
//! # use horizon_event_system::{create_horizon_event_system, RawClientMessageEvent};
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Example plugin handler registration
//! let event_system = create_horizon_event_system();
//! event_system.on_client("movement", "move_request", |event: RawClientMessageEvent, _player_id: horizon_event_system::PlayerId, _connection: horizon_event_system::ClientConnectionRef| {
//!     // Handle movement logic
//!     Ok(())
//! }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Configuration
//!
//! The server can be configured through the [`ServerConfig`] struct:
//!
//! * **Network settings** - Bind address, connection limits, timeouts
//! * **Region configuration** - Spatial bounds for the server region
//! * **Plugin management** - Plugin directory and loading behavior
//! * **Performance tuning** - Multi-threading and resource limits
//!
//! ## GORC Integration
//!
//! The server includes full GORC (Game Object Replication Channel) support:
//!
//! * **Spatial Partitioning** - Efficient proximity queries and region management
//! * **Subscription Management** - Dynamic event subscription based on player state
//! * **Multicast Groups** - Efficient broadcasting to groups of players
//! * **Replication Channels** - High-performance object state synchronization
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
//! * Event system provides async-safe handler registration and emission
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

// Internal modules (not part of public API)
mod connection;
mod messaging;
mod tests;

// Authentication integration tests
#[cfg(test)]
mod auth_integration_tests;