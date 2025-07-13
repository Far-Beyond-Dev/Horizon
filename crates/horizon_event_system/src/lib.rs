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
//!     let mut gorc_system = utils::create_complete_gorc_system(server_context)?;
//!     
//!     // Create event system with GORC integration
//!     let events = Arc::new(EventSystem::with_gorc(gorc_system.instance_manager.clone()));
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
pub mod context;
pub mod events;
pub mod macros;
pub mod plugin;
pub mod system;
pub mod system_tests;
pub mod types;
pub mod utils;

// GORC (Game Object Replication Channels) module
pub mod gorc;


// Re-export commonly used items for convenience
pub use context::{LogLevel, ServerContext, ServerError};
pub use events::{
    Event, EventError, EventHandler, GorcEvent, PlayerConnectedEvent, PlayerDisconnectedEvent,
    RawClientMessageEvent, RegionStartedEvent, RegionStoppedEvent, TypedEventHandler,
    PluginLoadedEvent, PluginUnloadedEvent,
};
pub use plugin::{Plugin, PluginError, SimplePlugin};
pub use system::{EventSystem, EventSystemStats, ClientConnectionRef, ClientResponseSender};
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
pub use crate::system::{DetailedEventSystemStats, HandlerCategoryStats};

// External dependencies that plugins commonly need
pub use async_trait::async_trait;
pub use std::sync::Arc;
pub use serde::{Deserialize, Serialize};

#[allow(unused_imports)] // This is actually used but only in the create plugin macro
pub use futures;

/// Creates a complete event system with full GORC integration
/// 
/// This is the recommended way to create an event system for games that need
/// object replication capabilities.
/// 
/// # Arguments
/// 
/// * `server_context` - Server context providing access to core services
/// 
/// # Returns
/// 
/// Returns a tuple of (EventSystem, CompleteGorcSystem) ready for use
/// 
/// # Examples
/// 
/// ```rust
/// let server_context = Arc::new(MyServerContext::new());
/// let (events, gorc_system) = create_complete_horizon_system(server_context)?;
/// 
/// // Use the event system for traditional events
/// events.on_core("server_started", |event: ServerStartedEvent| {
///     println!("Server online!");
///     Ok(())
/// }).await?;
/// 
/// // Use the GORC system for object replication
/// let asteroid_id = gorc_system.register_object(my_asteroid, position).await;
/// ```
pub fn create_complete_horizon_system(
    server_context: Arc<dyn ServerContext>
) -> Result<(Arc<EventSystem>, CompleteGorcSystem), gorc::GorcError> {
    let gorc_system = gorc::utils::create_complete_gorc_system(server_context)?;
    let event_system = Arc::new(EventSystem::with_gorc(gorc_system.instance_manager.clone()));

    Ok((event_system, gorc_system))
}

/// Creates a lightweight event system without GORC for simple use cases
/// 
/// This creates just the basic event system without object replication capabilities.
/// Use this for simpler applications that don't need advanced replication features.
/// 
/// # Returns
/// 
/// Returns an Arc<EventSystem> ready for basic event handling
/// 
/// # Examples
/// 
/// ```rust
/// let events = create_simple_horizon_system();
/// 
/// events.on_core("player_connected", |event: PlayerConnectedEvent| {
///     println!("Player {} connected", event.player_id);
///     Ok(())
/// }).await?;
/// ```
pub fn create_simple_horizon_system() -> Arc<EventSystem> {
    create_horizon_event_system()
}

/// Helper trait for easily implementing GORC objects
/// 
/// This trait provides default implementations for common GORC object patterns,
/// reducing boilerplate code for simple object types.
pub trait SimpleGorcObject: Clone + Send + Sync + std::fmt::Debug + 'static {
    /// Get the object's current position
    fn position(&self) -> Vec3;
    
    /// Update the object's position
    fn set_position(&mut self, position: Vec3);
    
    /// Get the object's type name
    fn object_type() -> &'static str where Self: Sized;
    
    /// Get properties to replicate for a given channel
    fn channel_properties(channel: u8) -> Vec<String> where Self: Sized;
    
    /// Get replication configuration for this object type
    fn replication_config() -> SimpleReplicationConfig where Self: Sized {
        SimpleReplicationConfig::default()
    }
}

/// Simple configuration for GORC objects
#[derive(Debug, Clone)]
pub struct SimpleReplicationConfig {
    /// Radius for each channel
    pub channel_radii: [f32; 4],
    /// Frequency for each channel
    pub channel_frequencies: [f32; 4],
    /// Compression for each channel
    pub channel_compression: [CompressionType; 4],
}

impl Default for SimpleReplicationConfig {
    fn default() -> Self {
        Self {
            channel_radii: [50.0, 150.0, 300.0, 1000.0],
            channel_frequencies: [30.0, 15.0, 10.0, 2.0],
            channel_compression: [
                CompressionType::Delta,
                CompressionType::Lz4,
                CompressionType::Lz4,
                CompressionType::High,
            ],
        }
    }
}

/// Automatic implementation of GorcObject for types implementing SimpleGorcObject
impl<T> GorcObject for T 
where 
    T: SimpleGorcObject + serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    fn type_name(&self) -> &'static str {
        T::object_type()
    }
    
    fn position(&self) -> Vec3 {
        SimpleGorcObject::position(self)
    }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position().distance(observer_pos);
        match distance {
            d if d < 100.0 => ReplicationPriority::Critical,
            d if d < 300.0 => ReplicationPriority::High,
            d if d < 1000.0 => ReplicationPriority::Normal,
            _ => ReplicationPriority::Low,
        }
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // For SimpleGorcObject, we serialize the entire object and let the layer filter properties
        let serialized = serde_json::to_value(self)?;
        
        if let serde_json::Value::Object(mut map) = serialized {
            // Keep only properties specified in the layer
            map.retain(|key, _| layer.properties.contains(key));
            Ok(serde_json::to_vec(&map)?)
        } else {
            Ok(serde_json::to_vec(&serialized)?)
        }
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        let config = T::replication_config();
        let mut layers = Vec::new();
        
        for channel in 0..4 {
            let properties = T::channel_properties(channel);
            if !properties.is_empty() {
                layers.push(ReplicationLayer::new(
                    channel,
                    config.channel_radii[channel as usize],
                    config.channel_frequencies[channel as usize],
                    properties,
                    config.channel_compression[channel as usize],
                ));
            }
        }
        
        layers
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.set_position(new_position);
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}

/// Macro for easily defining simple GORC objects
/// 
/// This macro reduces boilerplate by automatically implementing the SimpleGorcObject trait
/// and providing sensible defaults for most object types.
/// 
/// # Examples
/// 
/// ```rust
/// define_simple_gorc_object! {
///     struct MyAsteroid {
///         position: Vec3,
///         velocity: Vec3,
///         health: f32,
///         mineral_type: MineralType,
///     }
///     
///     type_name: "MyAsteroid",
///     
///     channels: {
///         0 => ["position", "health"],      // Critical
///         1 => ["velocity"],                // Detailed  
///         3 => ["mineral_type"],            // Metadata
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_simple_gorc_object {
    (
        struct $name:ident {
            position: Vec3,
            $($field:ident: $field_type:ty),*
        }
        
        type_name: $type_name:expr,
        
        channels: {
            $($channel:expr => [$($prop:expr),*]),*
        }
        
        $(config: {
            $($config_field:ident: $config_value:expr),*
        })?
    ) => {
        #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
        pub struct $name {
            pub position: Vec3,
            $(pub $field: $field_type),*
        }
        
        impl SimpleGorcObject for $name {
            fn position(&self) -> Vec3 {
                self.position
            }
            
            fn set_position(&mut self, position: Vec3) {
                self.position = position;
            }
            
            fn object_type() -> &'static str {
                $type_name
            }
            
            fn channel_properties(channel: u8) -> Vec<String> {
                match channel {
                    $(
                        $channel => vec![$($prop.to_string()),*],
                    )*
                    _ => vec![]
                }
            }
            
            $(
                fn replication_config() -> SimpleReplicationConfig {
                    let mut config = SimpleReplicationConfig::default();
                    $(
                        config.$config_field = $config_value;
                    )*
                    config
                }
            )?
        }
    };
}

/// Performance monitoring utilities
pub mod monitoring {
    use super::*;
    use std::time::{Duration, Instant};
    
    /// Performance monitor for the entire Horizon system
    pub struct HorizonMonitor {
        start_time: Instant,
        last_report: Instant,
        event_system: Arc<EventSystem>,
        gorc_system: Option<Arc<CompleteGorcSystem>>,
    }
    
    impl HorizonMonitor {
        /// Creates a new performance monitor
        pub fn new(event_system: Arc<EventSystem>) -> Self {
            Self {
                start_time: Instant::now(),
                last_report: Instant::now(),
                event_system,
                gorc_system: None,
            }
        }
        
        /// Creates a monitor with GORC system integration
        pub fn with_gorc(event_system: Arc<EventSystem>, gorc_system: Arc<CompleteGorcSystem>) -> Self {
            Self {
                start_time: Instant::now(),
                last_report: Instant::now(),
                event_system,
                gorc_system: Some(gorc_system),
            }
        }
        
        /// Generates a comprehensive system report
        pub async fn generate_report(&mut self) -> HorizonSystemReport {
            let now = Instant::now();
            let uptime = now.duration_since(self.start_time);
            let time_since_last = now.duration_since(self.last_report);
            self.last_report = now;
            
            let event_stats = self.event_system.as_ref().get_stats().await;
            let gorc_report = if let Some(ref gorc) = self.gorc_system {
                Some(gorc.get_performance_report().await)
            } else {
                None
            };
            
            HorizonSystemReport {
                timestamp: current_timestamp(),
                uptime_seconds: uptime.as_secs(),
                report_interval_seconds: time_since_last.as_secs(),
                event_system_stats: event_stats.clone(),
                gorc_performance: gorc_report.clone(),
                system_health: self.calculate_system_health(&event_stats, &gorc_report).await,
            }
        }
        
        /// Calculates overall system health score (0.0 to 1.0)
        async fn calculate_system_health(
            &self,
            event_stats: &EventSystemStats,
            gorc_report: &Option<GorcPerformanceReport>
        ) -> f32 {
            let mut health_score = 1.0;
            
            // Factor in event system health
            if event_stats.total_handlers == 0 {
                health_score -= 0.2; // No handlers is concerning
            }
            
            // Factor in GORC health if available
            if let Some(gorc) = gorc_report {
                let gorc_health = gorc.health_score();
                health_score = (health_score + gorc_health) / 2.0;
            }
            
            health_score.clamp(0.0, 1.0)
        }
        
        /// Checks if the system should trigger alerts
        pub async fn should_alert(&self) -> Vec<String> {
            let mut alerts = Vec::new();
            
            let event_stats = self.event_system.get_stats().await;
            
            // Check for event system issues
            if event_stats.total_handlers > 10000 {
                alerts.push("Very high number of event handlers registered".to_string());
            }
            
            // Check GORC system if available
            if let Some(ref gorc) = self.gorc_system {
                let gorc_report = gorc.get_performance_report().await;
                if !gorc_report.is_healthy() {
                    alerts.push("GORC system health issues detected".to_string());
                }
                
                if gorc_report.network_utilization > 0.9 {
                    alerts.push(format!("Critical network utilization: {:.1}%", 
                                      gorc_report.network_utilization * 100.0));
                }
            }
            
            alerts
        }
    }
    
    /// Comprehensive system health report
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct HorizonSystemReport {
        pub timestamp: u64,
        pub uptime_seconds: u64,
        pub report_interval_seconds: u64,
        pub event_system_stats: EventSystemStats,
        pub gorc_performance: Option<GorcPerformanceReport>,
        pub system_health: f32,
    }
    
    impl HorizonSystemReport {
        /// Returns true if the system is operating normally
        pub fn is_healthy(&self) -> bool {
            self.system_health > 0.7 && 
            (self.gorc_performance.as_ref().map(|g| g.is_healthy()).unwrap_or(true))
        }
        
        /// Gets actionable recommendations for system improvement
        pub fn get_recommendations(&self) -> Vec<String> {
            let mut recommendations = Vec::new();
            
            if self.system_health < 0.5 {
                recommendations.push("System health is poor - investigate event system and GORC performance".to_string());
            }
            
            if let Some(ref gorc) = self.gorc_performance {
                recommendations.extend(gorc.get_recommendations());
            }
            
            if self.event_system_stats.total_handlers == 0 {
                recommendations.push("No event handlers registered - system may not be functioning".to_string());
            }
            
            recommendations
        }
    }
}

// Export monitoring utilities
pub use monitoring::{HorizonMonitor, HorizonSystemReport};

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