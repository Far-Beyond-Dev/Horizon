//! # Horizon Event System
//!
//! A high-performance, type-safe event system designed for game servers with plugin architecture
//! and advanced Game Object Replication Channels (GORC) for multiplayer state management.
//!
//! ## Core Features
//!
//! - **Type Safety**: All events are strongly typed with compile-time guarantees
//! - **Async/Await Support**: Built on Tokio for high-performance async operations
//! - **Plugin Architecture**: Clean separation between core server and plugin events
//! - **GORC Integration**: Advanced object replication with zone-based subscriptions
//! - **Instance Management**: Object-specific events with direct instance access
//! - **Network Optimization**: Intelligent batching, compression, and priority queuing
//! - **Spatial Awareness**: Efficient proximity-based replication and subscriptions
//! - **Performance Monitoring**: Comprehensive statistics and health reporting
//!
//! ## Architecture Overview
//!
//! The system is organized into several integrated components:
//!
//! ### Core Event System
//! - **Core Events** (`core:*`): Server infrastructure events
//! - **Client Events** (`client:namespace:event`): Messages from game clients  
//! - **Plugin Events** (`plugin:plugin_name:event`): Inter-plugin communication
//! - **GORC Events** (`gorc:object_type:channel:event`): Object replication events
//! - **Instance Events** (`gorc_instance:object_type:channel:event`): Instance-specific events
//!
//! ### GORC Replication System
//! - **Object Instances**: Individual game objects with unique zones
//! - **Zone Management**: Multi-channel proximity-based replication
//! - **Network Engine**: Optimized batching and delivery
//! - **Subscription System**: Dynamic player subscription management
//!
//! ## Quick Start Example
//!
//! ```rust,no_run
//! use horizon_event_system::*;
//! use std::sync::Arc;
//!
//! // Mock server context for example
//! struct MyServerContext;
//! impl context::ServerContext for MyServerContext {}
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create the complete GORC system
//!     let server_context = Arc::new(MyServerContext);
//!     let (events, mut gorc_system) = create_complete_horizon_system(server_context)?;
//!     
//!     // Register event handlers
//!     events.on_core("player_connected", |event: PlayerConnectedEvent| {
//!         println!("Player {} connected", event.player_id);
//!         Ok(())
//!     }).await?;
//!     
//!     // Register GORC instance handlers with object access
//!     events.on_gorc_instance("Asteroid", 0, "position_update", 
//!         |event: GorcEvent, instance: &mut ObjectInstance| {
//!             if let Some(asteroid) = instance.get_object_mut::<gorc::examples::ExampleAsteroid>() {
//!                 println!("Asteroid {} moved to {:?}", event.object_id, asteroid.position());
//!             }
//!             Ok(())
//!         }
//!     ).await?;
//!     
//!     // Register game objects
//!     let asteroid = gorc::examples::ExampleAsteroid::new(Vec3::new(100.0, 0.0, 200.0), gorc::MineralType::Platinum);
//!     let asteroid_id = gorc_system.register_object(asteroid, Vec3::new(100.0, 0.0, 200.0)).await;
//!     
//!     // Add players
//!     let player1_id = PlayerId::new();
//!     gorc_system.add_player(player1_id, Vec3::new(50.0, 0.0, 180.0)).await;
//!     
//!     // Run the main game loop
//!     loop {
//!         // Process GORC replication
//!         gorc_system.tick().await?;
//!         
//!         // Emit core events
//!         events.emit_core("tick", &ServerTickEvent {
//!             tick_number: 12345,
//!             timestamp: current_timestamp(),
//!         }).await?;
//!         
//!         tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60 FPS
//!     }
//! }
//! ```
//!
//! ## Plugin Development with GORC
//!
//! ```rust,no_run
//! use horizon_event_system::*;
//! use std::sync::Arc;
//!
//! struct AsteroidMiningPlugin {
//!     gorc_system: Arc<gorc::CompleteGorcSystem>,
//! }
//!
//! impl AsteroidMiningPlugin {
//!     fn new(gorc_system: Arc<gorc::CompleteGorcSystem>) -> Self {
//!         Self { gorc_system }
//!     }
//! }
//!
//! #[async_trait::async_trait]
//! impl plugin::SimplePlugin for AsteroidMiningPlugin {
//!     fn name(&self) -> &str { "asteroid_mining" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     
//!     async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), plugin::PluginError> {
//!         // Handle asteroid discovery events
//!         events.on_gorc_instance("Asteroid", 3, "composition_discovered", 
//!             |event: GorcEvent, instance: &mut ObjectInstance| {
//!                 if let Some(asteroid) = instance.get_object::<gorc::examples::ExampleAsteroid>() {
//!                     println!("Discovered {:?} asteroid with {} minerals", 
//!                             asteroid.mineral_type, asteroid.radius);
//!                 }
//!                 Ok(())
//!             }
//!         ).await?;
//!         
//!         // Handle player mining actions
//!         events.on_client("mining", "start_mining", |event: RawClientMessageEvent| {
//!             // Process mining start request
//!             Ok(())
//!         }).await?;
//!         
//!         Ok(())
//!     }
//! }
//!
//! // create_simple_plugin!(AsteroidMiningPlugin);
//! ```

// tests
mod test_integration;

// auth tests
#[cfg(test)]
mod auth_tests;

// Core modules
pub mod api;
pub mod async_logging;
pub mod context;
pub mod events;
pub mod gorc_macros;
pub mod macros;
pub mod monitoring;
pub mod plugin;
pub mod shutdown;
pub mod system;
pub mod traits;
pub mod types;
pub mod utils;

// GORC (Game Object Replication Channels) module
pub mod gorc;

// Re-export commonly used items for convenience
pub use api::{create_complete_horizon_system, create_simple_horizon_system};
pub use utils::{create_horizon_event_system, current_timestamp};
pub use traits::{SimpleGorcObject, SimpleReplicationConfig};
pub use gorc_macros::{GorcZoneData, __get_default_zone_config}; // Export new type-based system
pub use monitoring::{HorizonMonitor, HorizonSystemReport};
pub use context::{LogLevel, ServerContext, ServerError};
pub use plugin::{Plugin, PluginError, SimplePlugin};
pub use shutdown::ShutdownState;
pub use types::*;

pub use events::{
    Event, EventError, EventHandler, GorcEvent, Dest,
    PlayerConnectedEvent, PlayerDisconnectedEvent,
    PlayerMovementEvent, RawClientMessageEvent, 
    RegionStartedEvent, RegionStoppedEvent, TypedEventHandler,
    PluginLoadedEvent, PluginUnloadedEvent,
    AuthenticationStatusGetResponseEvent,
    AuthenticationStatusChangedEvent,
    AuthenticationStatusSetEvent,
    AuthenticationStatusGetEvent,
    ClientEventWrapper,
};

pub use system::{
    EventSystem, EventSystemStats,
    DetailedEventSystemStats,
    HandlerCategoryStats,
    ClientConnectionRef,
    ClientResponseSender,
    ClientConnectionInfo
};

// Re-export GORC components for easy access
pub use gorc::{
    // Core GORC types
    GorcObject, GorcObjectId, ObjectInstance, GorcInstanceManager,
    
    // Channels and layers
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, 
    CompressionType, GorcManager, GorcConfig, GorcStats, PerformanceReport,
    
    // Zones and spatial management
    ObjectZone, ZoneManager, ZoneAnalysis, ZoneConfig, 
    SpatialPartition, SpatialQuery, RegionRTree,
    
    // Network and replication
    NetworkReplicationEngine, ReplicationCoordinator, NetworkConfig, 
    NetworkStats, ReplicationUpdate, ReplicationBatch, ReplicationStats,
    Replication, GorcObjectRegistry,
    
    // Subscription management
    SubscriptionManager, SubscriptionType, ProximitySubscription,
    RelationshipSubscription, InterestSubscription, InterestLevel,
    
    // Multicast and LOD
    MulticastManager, MulticastGroup, LodRoom, LodLevel, MulticastGroupId,
    
    // Utilities and examples
    CompleteGorcSystem, GorcPerformanceReport, MineralType,
    
    // Example implementations
    examples::{ExampleAsteroid, ExamplePlayer, ExampleProjectile, TypedAsteroid, TypedPlayer, TypedProjectile},
    
    // Utility functions
    defaults,
    
    // Constants
    GORC_VERSION, MAX_CHANNELS,
};

// External dependencies that plugins commonly need
pub use async_trait::async_trait;
pub use std::sync::Arc;
pub use serde::{Deserialize, Serialize};
pub use futures;

/// ABI version for plugin compatibility validation.
/// This is derived from the crate version and Rust compiler version to ensure plugins are compatible.
/// Format: "major.minor.patch:rust_version"
/// Example: "0.10.0:1.75.0" or "0.10.0:unknown"
pub const ABI_VERSION: &str = {
    const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");
    
    // Use the Rust version detected by our build script
    // This will be set by build.rs after attempting to detect the actual Rust version
    const RUST_VERSION: &str = env!("HORIZON_RUSTC_VERSION");
    
    // Create a compile-time concatenated string with format "crate_version:rust_version"
    const_format::concatcp!(CRATE_VERSION, ":", RUST_VERSION)
};

/// Returns build info string with version and Rust compiler version (if available)
pub fn horizon_build_info() -> String {
    format!(
        "Horizon Event System v{} with Rust compiler v{}",
        env!("CARGO_PKG_VERSION"),
        env!("HORIZON_RUSTC_VERSION")
    )
}