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
//! ```rust
//! use horizon_event_system::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create the complete GORC system
//!     let server_context = Arc::new(MyServerContext::new());
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
//!             if let Some(asteroid) = instance.get_object_mut::<ExampleAsteroid>() {
//!                 println!("Asteroid {} moved to {:?}", event.object_id, asteroid.position());
//!             }
//!             Ok(())
//!         }
//!     ).await?;
//!     
//!     // Register game objects
//!     let asteroid = ExampleAsteroid::new(Vec3::new(100.0, 0.0, 200.0), MineralType::Platinum);
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
//! ```rust
//! use horizon_event_system::*;
//!
//! struct AsteroidMiningPlugin {
//!     gorc_system: Arc<CompleteGorcSystem>,
//! }
//!
//! impl AsteroidMiningPlugin {
//!     fn new(gorc_system: Arc<CompleteGorcSystem>) -> Self {
//!         Self { gorc_system }
//!     }
//! }
//!
//! #[async_trait]
//! impl SimplePlugin for AsteroidMiningPlugin {
//!     fn name(&self) -> &str { "asteroid_mining" }
//!     fn version(&self) -> &str { "1.0.0" }
//!     
//!     async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
//!         // Handle asteroid discovery events
//!         events.on_gorc_instance("Asteroid", 3, "composition_discovered", 
//!             |event: GorcEvent, instance: &mut ObjectInstance| {
//!                 if let Some(asteroid) = instance.get_object::<ExampleAsteroid>() {
//!                     println!("Discovered {} asteroid with {} minerals", 
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
//! create_simple_plugin!(AsteroidMiningPlugin);
//! ```

// Core modules
pub mod api;
pub mod context;
pub mod events;
pub mod gorc_macros;
pub mod macros;
pub mod monitoring;
pub mod plugin;
pub mod system;
pub mod system_tests;
pub mod traits;
pub mod types;
pub mod utils;

// GORC (Game Object Replication Channels) module
pub mod gorc;


// Re-export commonly used items for convenience
pub use api::{create_complete_horizon_system, create_simple_horizon_system};
pub use context::{LogLevel, ServerContext, ServerError};
pub use events::{
    Event, EventError, EventHandler, GorcEvent, PlayerConnectedEvent, PlayerDisconnectedEvent,
    RawClientMessageEvent, RegionStartedEvent, RegionStoppedEvent, TypedEventHandler,
    PluginLoadedEvent, PluginUnloadedEvent,
};
pub use monitoring::{HorizonMonitor, HorizonSystemReport};
pub use plugin::{Plugin, PluginError, SimplePlugin};
pub use system::{EventSystem, EventSystemStats, DetailedEventSystemStats, HandlerCategoryStats, ClientConnectionRef, ClientResponseSender};
pub use traits::{SimpleGorcObject, SimpleReplicationConfig};
pub use types::*;
pub use utils::{create_horizon_event_system, current_timestamp};

// Re-export GORC components for easy access
pub use gorc::{
    // Core GORC types
    GorcObject, GorcObjectId, ObjectInstance, GorcInstanceManager,
    
    // Channels and layers
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, 
    CompressionType, GorcManager, GorcConfig, GorcStats, PerformanceReport,
    
    // Zones and spatial management
    ObjectZone, ZoneManager, ZoneAnalysis, ZoneConfig, 
    SpatialPartition, SpatialQuery, RegionQuadTree,
    
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
    examples::{ExampleAsteroid, ExamplePlayer},
    
    // Utility functions
    defaults,
    
    // Constants
    GORC_VERSION, MAX_CHANNELS,
};

// External dependencies that plugins commonly need
pub use async_trait::async_trait;
pub use std::sync::Arc;
pub use serde::{Deserialize, Serialize};

#[allow(unused_imports)] // This is actually used but only in the create plugin macro
pub use futures;


/// Version information
pub const HORIZON_VERSION: &str = "2.0.0";
pub const HORIZON_BUILD_INFO: &str = concat!(
    "Horizon Event System v", 
    env!("CARGO_PKG_VERSION"),
    " with GORC v",
    "1.0.0"
);

#[cfg(test)]
mod integration_tests {
    use super::*;
    
    // Mock server context for testing
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct MockServerContext;
    
    impl MockServerContext {
        fn new() -> Self { Self }
    }
    
    #[async_trait]
    impl ServerContext for MockServerContext {
        fn events(&self) -> Arc<crate::system::EventSystem> {
            Arc::new(EventSystem::new())
        }
        
        fn region_id(&self) -> RegionId {
            RegionId::new()
        }
        
        fn log(&self, _level: LogLevel, _message: &str) {
            // Mock implementation
        }
        
        async fn send_to_player(&self, _player_id: PlayerId, _data: &[u8]) -> Result<(), ServerError> {
            Ok(())
        }
        
        async fn broadcast(&self, _data: &[u8]) -> Result<(), ServerError> {
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_complete_system_integration() {
        let server_context = Arc::new(MockServerContext::new());
        let (events, mut gorc_system) = create_complete_horizon_system(server_context).unwrap();
        
        // Test event registration
        events.on_core("test_event", |_: PlayerConnectedEvent| Ok(())).await.unwrap();
        
        // Test GORC object registration
        let asteroid = ExampleAsteroid::new(Vec3::new(100.0, 0.0, 200.0), MineralType::Platinum);
        let _asteroid_id = gorc_system.register_object(asteroid, Vec3::new(100.0, 0.0, 200.0)).await;
        
        // Test player management
        let player_id = PlayerId::new();
        gorc_system.add_player(player_id, Vec3::new(50.0, 0.0, 180.0)).await;
        
        // Test system tick
        let result = gorc_system.tick().await;
        assert!(result.is_ok());
        
        // Test statistics
        let stats = gorc_system.get_stats().await;
        assert!(stats.instances.total_objects > 0);
    }
    
    #[tokio::test]
    async fn test_monitoring_system() {
        let events = create_simple_horizon_system();
        let mut monitor = HorizonMonitor::new(events.clone());
        
        // Generate initial report
        let report = monitor.generate_report().await;
        assert!(report.timestamp > 0);
        assert_eq!(report.uptime_seconds, 0); // Just started
        
        // Check alerts (should be none for new system)
        let alerts = monitor.should_alert().await;
        assert!(alerts.is_empty());
    }
}