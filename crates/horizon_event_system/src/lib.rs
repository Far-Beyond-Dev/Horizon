//! # Horizon Event System
//!
//! A high-performance, type-safe event system designed for game servers with plugin architecture.
//! This system provides a clean separation between core server events and plugin-specific events,
//! enabling modular game development while maintaining excellent performance and safety.
//!
//! ## Core Features
//!
//! - **Type Safety**: All events are strongly typed with compile-time guarantees
//! - **Async/Await Support**: Built on Tokio for high-performance async operations
//! - **Plugin Architecture**: Clean separation between core server and plugin events
//! - **Namespace Management**: Organized event routing with namespaces
//! - **Panic Safety**: Comprehensive panic handling for plugin FFI boundaries
//! - **Serialization**: Built-in JSON serialization for network transmission
//! - **Statistics**: Built-in performance monitoring and statistics
//!
//! ## Architecture Overview
//!
//! The event system is organized into three main categories:
//!
//! ### Core Events (`core:*`)
//! Server infrastructure events like player connections, plugin lifecycle, and region management.
//! These events are handled by the core server and are essential for basic operation.
//!
//! ### Client Events (`client:namespace:event`)
//! Events originating from game clients, organized by namespace (e.g., `movement`, `chat`, `ui`).
//! These events are typically forwarded to appropriate plugins for handling.
//!
//! ### Plugin Events (`plugin:plugin_name:event`)
//! Inter-plugin communication events that allow plugins to communicate with each other
//! without tight coupling.
//!
//! ## Quick Start Example
//!
//! ```rust
//! use horizon_event_system::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let events = create_horizon_event_system();
//!     
//!     // Register a core event handler
//!     events.on_core("player_connected", |event: PlayerConnectedEvent| {
//!         println!("Player {} connected", event.player_id);
//!         Ok(())
//!     }).await?;
//!     
//!     // Register client event handlers
//!     events.on_client("movement", "player_moved", |event: RawClientMessageEvent| {
//!         // Handle player movement
//!         Ok(())
//!     }).await?;
//!     
//!     // Emit events
//!     events.emit_core("player_connected", &PlayerConnectedEvent {
//!         player_id: PlayerId::new(),
//!         connection_id: "conn_123".to_string(),
//!         remote_addr: "127.0.0.1:8080".to_string(),
//!         timestamp: current_timestamp(),
//!     }).await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Plugin Development
//!
//! Create plugins using the `SimplePlugin` trait and `create_simple_plugin!` macro:
//!
//! ```rust
//! use horizon_event_system::*;
//!
//! struct MyGamePlugin {
//!     // Plugin state
//! }
//!
//! impl MyGamePlugin {
//!     fn new() -> Self {
//!         Self {}
//!     }
//! }
//!
//! #[async_trait]
//! impl SimplePlugin for MyGamePlugin {
//!     fn name(&self) -> &str { "my_game_plugin" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     
//!     async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
//!         register_handlers!(events;
//!             client {
//!                 "movement", "jump" => |event: RawClientMessageEvent| {
//!                     // Handle jump
//!                     Ok(())
//!                 }
//!             }
//!             core {
//!                 "player_connected" => |event: PlayerConnectedEvent| {
//!                     // Welcome new player
//!                     Ok(())
//!                 }
//!             }
//!         );
//!         Ok(())
//!     }
//! }
//!
//! create_simple_plugin!(MyGamePlugin);
//! ```

// Core modules
pub mod types;
pub mod events;
pub mod system;
pub mod plugin;
pub mod context;
pub mod utils;
pub mod macros;

// GORC (Game Object Replication Channels) module
pub mod gorc;

// Re-export commonly used items for convenience
pub use types::*;
pub use events::{Event, EventHandler, TypedEventHandler, EventError, PlayerConnectedEvent,
                 PlayerDisconnectedEvent, RawClientMessageEvent, RegionStartedEvent,
                 RegionStoppedEvent, GorcEvent};
pub use system::{EventSystem, EventSystemStats};
pub use plugin::{SimplePlugin, Plugin, PluginError};
pub use context::{ServerContext, LogLevel, ServerError};
pub use utils::{current_timestamp, create_horizon_event_system};

// Re-export GORC components
pub use gorc::{
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, CompressionType, 
    GorcManager, SubscriptionManager, SubscriptionType, ProximitySubscription, RelationshipSubscription,
    InterestSubscription, MulticastManager, MulticastGroup, SpatialPartition, SpatialQuery, 
    MineralType, Replication, GorcObjectRegistry
};

// External dependencies that plugins commonly need
pub use async_trait::async_trait;
pub use std::sync::Arc;

#[allow(unused_imports)] // This is actually used but only in the create plugin macro
pub use futures;