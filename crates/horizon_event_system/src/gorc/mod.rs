//! # Game Object Replication Channels (GORC)
//!
//! An advanced replication system for managing complex multiplayer game state distribution.
//! GORC provides fine-grained control over what information reaches which players and at
//! what frequency through a multi-channel architecture with instance-based zone management.
//!
//! ## Core Concepts
//!
//! ### Object Instances
//! Each game object is registered as a unique instance with its own replication zones.
//! Objects implement the `GorcObject` trait to define their replication behavior.
//!
//! ### Zone-Based Replication
//! Each object instance has multiple concentric zones corresponding to different channels:
//! - **Channel 0 (Critical)**: Immediate vicinity, 30-60Hz updates
//! - **Channel 1 (Detailed)**: Close interaction range, 15-30Hz updates  
//! - **Channel 2 (Cosmetic)**: Visual range, 5-15Hz updates
//! - **Channel 3 (Metadata)**: Strategic information, 1-5Hz updates
//!
//! ### Dynamic Subscriptions
//! Players are automatically subscribed/unsubscribed from object zones as they move,
//! ensuring optimal bandwidth usage and relevant information delivery.
//!
//! ## Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         GORC System                         │
//! │  ┌─────────────────┐  ┌──────────────────┐  ┌─────────────┐ │
//! │  │ Instance Manager│  │   Zone Manager   │  │   Network   │ │
//! │  │   - Objects     │  │   - Proximity    │  │  - Batching │ │
//! │  │   - Registry    │  │   - Hysteresis   │  │  - Priority │ │
//! │  │   - Lifecycle   │  │   - Subscriptions│  │  - Delivery │ │
//! │  └─────────────────┘  └──────────────────┘  └─────────────┘ │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage Examples
//!
//! ### Implementing a Replicable Object
//!
//! ```rust
//! use horizon_event_system::*;
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct Asteroid {
//!     position: Vec3,
//!     velocity: Vec3,
//!     health: f32,
//!     mineral_type: MineralType,
//! }
//!
//! impl GorcObject for Asteroid {
//!     fn type_name(&self) -> &'static str { "Asteroid" }
//!     
//!     fn position(&self) -> Vec3 { self.position }
//!     
//!     fn get_layers(&self) -> Vec<ReplicationLayer> {
//!         vec![
//!             // Critical layer for collision-relevant data
//!             ReplicationLayer::new(0, 50.0, 30.0, 
//!                 vec!["position".to_string(), "health".to_string()],
//!                 CompressionType::Delta),
//!             
//!             // Detailed layer for visual feedback
//!             ReplicationLayer::new(1, 150.0, 15.0,
//!                 vec!["velocity".to_string()],
//!                 CompressionType::Lz4),
//!             
//!             // Metadata layer for strategic information
//!             ReplicationLayer::new(3, 1000.0, 5.0,
//!                 vec!["mineral_type".to_string()],
//!                 CompressionType::High),
//!         ]
//!     }
//!     
//!     fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
//!         let distance = self.position.distance(observer_pos);
//!         if distance < 100.0 { ReplicationPriority::Critical }
//!         else if distance < 500.0 { ReplicationPriority::High }
//!         else { ReplicationPriority::Normal }
//!     }
//!     
//!     fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
//!         let mut data = serde_json::Map::new();
//!         
//!         // Only serialize properties relevant to this layer
//!         for property in &layer.properties {
//!             match property.as_str() {
//!                 "position" => {
//!                     data.insert("position".to_string(), serde_json::to_value(&self.position)?);
//!                 }
//!                 "velocity" => {
//!                     data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
//!                 }
//!                 "health" => {
//!                     data.insert("health".to_string(), serde_json::to_value(self.health)?);
//!                 }
//!                 "mineral_type" => {
//!                     data.insert("mineral_type".to_string(), serde_json::to_value(&self.mineral_type)?);
//!                 }
//!                 _ => {} // Ignore unknown properties
//!             }
//!         }
//!         
//!         Ok(serde_json::to_vec(&data)?)
//!     }
//!     
//!     fn update_position(&mut self, new_position: Vec3) {
//!         self.position = new_position;
//!     }
//!     
//!     fn as_any(&self) -> &dyn Any { self }
//!     fn as_any_mut(&mut self) -> &mut dyn Any { self }
//!     
//!     fn clone_object(&self) -> Box<dyn GorcObject> {
//!         Box::new(self.clone())
//!     }
//! }
//! ```
//!
//! ### Setting Up the GORC System
//!
//! ```rust
//! use horizon_event_system::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create the GORC system components
//!     let instance_manager = Arc::new(GorcInstanceManager::new());
//!     let network_config = NetworkConfig::default();
//!     let server_context = Arc::new(TestServerContext::new()); // Your server context
//!     
//!     let network_engine = Arc::new(NetworkReplicationEngine::new(
//!         network_config,
//!         instance_manager.clone(),
//!         server_context,
//!     ));
//!     
//!     let coordinator = ReplicationCoordinator::new(
//!         network_engine,
//!         instance_manager.clone(),
//!     );
//!     
//!     // Register an asteroid
//!     let asteroid = Asteroid {
//!         position: Vec3::new(100.0, 0.0, 200.0),
//!         velocity: Vec3::new(5.0, 0.0, -2.0),
//!         health: 100.0,
//!         mineral_type: MineralType::Platinum,
//!     };
//!     
//!     let asteroid_id = coordinator.register_object(asteroid, Vec3::new(100.0, 0.0, 200.0)).await;
//!     
//!     // Add players
//!     coordinator.add_player(player1_id, Vec3::new(50.0, 0.0, 180.0)).await;
//!     coordinator.add_player(player2_id, Vec3::new(500.0, 0.0, 500.0)).await;
//!     
//!     // Run the replication loop
//!     let mut coordinator = coordinator;
//!     loop {
//!         coordinator.tick().await?;
//!         tokio::time::sleep(tokio::time::Duration::from_millis(16)).await; // ~60 FPS
//!     }
//! }
//! ```
//!
//! ### Event Handling
//!
//! ```rust
//! // Register handlers for object instance events
//! events.on_gorc_instance("Asteroid", 0, "position_update", 
//!     |event: GorcEvent, instance: &mut ObjectInstance| {
//!         if let Some(asteroid) = instance.get_object_mut::<Asteroid>() {
//!             println!("Asteroid {} moved to {:?}", event.object_id, asteroid.position());
//!         }
//!         Ok(())
//!     }
//! ).await?;
//!
//! // Emit events for specific object instances
//! events.emit_gorc_instance(asteroid_id, 0, "position_update", &gorc_event).await?;
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Sub-millisecond event routing** for critical channels
//! - **Efficient spatial partitioning** with O(log n) proximity queries
//! - **Adaptive frequency scaling** based on network conditions
//! - **Intelligent subscription management** with hysteresis for stability
//! - **Comprehensive statistics** for monitoring and optimization

pub mod channels;
pub mod instance;
pub mod zones;
pub mod network;
pub mod subscription;
pub mod multicast;
pub mod spatial;

use std::sync::Arc;

// Re-export core types for convenience
pub use channels::{
    ReplicationChannel, ReplicationLayer, ReplicationLayers, ReplicationPriority, 
    CompressionType, GorcManager, MineralType, Replication, GorcObjectRegistry,
    GorcConfig, GorcStats, PerformanceReport, GorcError
};

pub use instance::{
    GorcObject, GorcObjectId, ObjectInstance, GorcInstanceManager, 
    InstanceManagerStats, ObjectStats
};

pub use zones::{
    ObjectZone, ZoneManager, ZoneAnalysis, ZoneConfig, ZoneStats,
    AdvancedZoneManager, ZonePerformanceMetrics
};

pub use network::{
    NetworkReplicationEngine, ReplicationCoordinator, NetworkConfig, NetworkStats,
    ReplicationUpdate, ReplicationBatch, ReplicationStats, NetworkError,
    UpdateScheduler, SchedulerStats
};

pub use subscription::{
    SubscriptionManager, SubscriptionType, ProximitySubscription,
    RelationshipSubscription, InterestSubscription, SubscriptionStats,
    InterestLevel, ActivityPattern
};

pub use multicast::{
    MulticastManager, MulticastGroup, LodRoom, LodLevel, MulticastGroupId,
    GroupBounds, MulticastStats, MulticastError
};

pub use spatial::{
    SpatialPartition, SpatialQuery, RegionQuadTree, QueryResult, QueryFilters,
    SpatialStats, GlobalSpatialStats
};

/// Current version of the GORC system
pub const GORC_VERSION: &str = "1.0.0";

/// Maximum number of replication channels supported
pub const MAX_CHANNELS: u8 = 4;

/// Default channel configurations
pub mod defaults {
    use super::*;
    
    /// Creates default replication layers for a typical game object
    pub fn default_object_layers() -> ReplicationLayers {
        let mut layers = ReplicationLayers::new();
        
        layers.add_layer(ReplicationLayer::new(
            0, 50.0, 30.0,
            vec!["position".to_string(), "health".to_string()],
            CompressionType::Delta
        ));
        
        layers.add_layer(ReplicationLayer::new(
            1, 150.0, 15.0,
            vec!["animation".to_string(), "state".to_string()],
            CompressionType::Lz4
        ));
        
        layers.add_layer(ReplicationLayer::new(
            2, 300.0, 10.0,
            vec!["effects".to_string()],
            CompressionType::Lz4
        ));
        
        layers.add_layer(ReplicationLayer::new(
            3, 1000.0, 2.0,
            vec!["name".to_string(), "metadata".to_string()],
            CompressionType::High
        ));
        
        layers
    }
    
    /// Creates default network configuration optimized for most games
    pub fn default_network_config() -> NetworkConfig {
        NetworkConfig {
            max_bandwidth_per_player: 512 * 1024, // 512 KB/s per player
            max_batch_size: 25,
            max_batch_age_ms: 16, // ~60 FPS
            target_frequencies: {
                let mut freq = std::collections::HashMap::new();
                freq.insert(0, 30.0); // Critical - 30Hz
                freq.insert(1, 15.0); // Detailed - 15Hz
                freq.insert(2, 10.0); // Cosmetic - 10Hz
                freq.insert(3, 2.0);  // Metadata - 2Hz
                freq
            },
            compression_enabled: true,
            compression_threshold: 128,
            priority_queue_sizes: {
                let mut sizes = std::collections::HashMap::new();
                sizes.insert(ReplicationPriority::Critical, 500);
                sizes.insert(ReplicationPriority::High, 250);
                sizes.insert(ReplicationPriority::Normal, 100);
                sizes.insert(ReplicationPriority::Low, 50);
                sizes
            },
        }
    }
    
    /// Creates default zone configuration for balanced performance
    pub fn default_zone_config() -> ZoneConfig {
        ZoneConfig {
            hysteresis_factor: 0.1, // 10% hysteresis to prevent flapping
            min_update_interval_ms: 33, // ~30 FPS minimum
            max_subscribers_per_zone: 50,
            adaptive_sizing: true,
            adaptive_scale_factor: 0.2,
        }
    }
}

/// Utility functions for GORC system setup and management
pub mod utils {
    use super::*;
    use crate::context::ServerContext;
    use std::sync::Arc;
    
    /// Creates a complete GORC system with all components configured
    pub fn create_complete_gorc_system(
        server_context: Arc<dyn ServerContext>
    ) -> Result<CompleteGorcSystem, GorcError> {
        let instance_manager = Arc::new(GorcInstanceManager::new());
        let network_config = defaults::default_network_config();
        
        let network_engine = Arc::new(NetworkReplicationEngine::new(
            network_config,
            instance_manager.clone(),
            server_context,
        ));
        
        let coordinator = ReplicationCoordinator::new(
            network_engine.clone(),
            instance_manager.clone(),
        );
        
        Ok(CompleteGorcSystem {
            instance_manager,
            network_engine,
            coordinator,
        })
    }
    
    /// Validates a GORC system configuration
    pub async fn validate_gorc_system(system: &CompleteGorcSystem) -> Vec<String> {
        let mut issues = Vec::new();
        
        // Check instance manager
        let instance_stats = system.instance_manager.get_stats().await;
        if instance_stats.total_objects == 0 {
            issues.push("No objects registered in GORC system".to_string());
        }
        
        if instance_stats.total_objects > 10000 {
            issues.push(format!("Very high object count: {}", instance_stats.total_objects));
        }
        
        // Check network engine
        let network_stats = system.network_engine.get_stats().await;
        let utilization = network_stats.network_utilization;
        
        if utilization > 0.9 {
            issues.push(format!("High network utilization: {:.1}%", utilization * 100.0));
        }
        
        if network_stats.updates_dropped > 0 {
            issues.push(format!("Updates being dropped: {}", network_stats.updates_dropped));
        }
        
        issues
    }
    
    /// Creates a performance monitoring report for the GORC system
    pub async fn create_performance_report(system: &CompleteGorcSystem) -> GorcPerformanceReport {
        let replication_stats = system.coordinator.get_stats().await;
        let instance_stats = system.instance_manager.get_stats().await;
        let network_stats = system.network_engine.get_stats().await;
        let utilization = network_stats.network_utilization;
        
        GorcPerformanceReport {
            timestamp: crate::utils::current_timestamp(),
            total_objects: instance_stats.total_objects,
            total_subscriptions: instance_stats.total_subscriptions,
            network_utilization: utilization,
            events_sent: network_stats.updates_sent,
            bytes_transmitted: network_stats.bytes_transmitted,
            updates_dropped: network_stats.updates_dropped,
            avg_batch_size: network_stats.avg_batch_size,
            issues: validate_gorc_system(system).await,
        }
    }
}

/// Complete GORC system with all components
#[derive(Debug, Clone)]
pub struct CompleteGorcSystem {
    /// Instance manager for object lifecycle
    pub instance_manager: Arc<GorcInstanceManager>,
    /// Network engine for replication
    pub network_engine: Arc<NetworkReplicationEngine>,
    /// Coordinator that ties everything together
    pub coordinator: ReplicationCoordinator,
}

impl CompleteGorcSystem {
    /// Registers a new object with the GORC system
    pub async fn register_object<T: GorcObject + 'static>(
        &mut self,
        object: T,
        position: crate::types::Vec3,
    ) -> GorcObjectId {
        self.coordinator.register_object(object, position).await
    }
    
    /// Unregisters an object from the GORC system
    pub async fn unregister_object(&mut self, object_id: GorcObjectId) {
        self.coordinator.unregister_object(object_id).await;
    }
    
    /// Adds a player to the system
    pub async fn add_player(&self, player_id: crate::types::PlayerId, position: crate::types::Vec3) {
        self.coordinator.add_player(player_id, position).await;
    }
    
    /// Removes a player from the system
    pub async fn remove_player(&self, player_id: crate::types::PlayerId) {
        self.coordinator.remove_player(player_id).await;
    }
    
    /// Updates a player's position
    pub async fn update_player_position(&self, player_id: crate::types::PlayerId, position: crate::types::Vec3) {
        self.coordinator.update_player_position(player_id, position).await;
    }
    
    /// Runs one tick of the replication system
    pub async fn tick(&mut self) -> Result<(), NetworkError> {
        self.coordinator.tick().await
    }
    
    /// Gets comprehensive system statistics
    pub async fn get_stats(&self) -> ReplicationStats {
        self.coordinator.get_stats().await
    }
    
    /// Gets a performance report
    pub async fn get_performance_report(&self) -> GorcPerformanceReport {
        utils::create_performance_report(self).await
    }
}

/// Comprehensive performance report for the GORC system
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GorcPerformanceReport {
    /// Report timestamp
    pub timestamp: u64,
    /// Total number of registered objects
    pub total_objects: usize,
    /// Total active subscriptions
    pub total_subscriptions: usize,
    /// Network utilization (0.0 to 1.0)
    pub network_utilization: f32,
    /// Total replication events sent
    pub events_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Number of updates dropped due to bandwidth limits
    pub updates_dropped: u64,
    /// Average batch size
    pub avg_batch_size: f32,
    /// System issues detected
    pub issues: Vec<String>,
}

impl GorcPerformanceReport {
    /// Checks if the system is performing well
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty() && 
        self.network_utilization < 0.8 && 
        self.updates_dropped == 0
    }
    
    /// Gets a health score from 0.0 (poor) to 1.0 (excellent)
    pub fn health_score(&self) -> f32 {
        let mut score = 1.0;
        
        // Penalize high network utilization
        if self.network_utilization > 0.5 {
            score -= (self.network_utilization - 0.5) * 0.5;
        }
        
        // Penalize dropped updates
        if self.updates_dropped > 0 {
            score -= 0.2;
        }
        
        // Penalize issues
        score -= (self.issues.len() as f32 * 0.1).min(0.5);
        
        score.max(0.0)
    }
    
    /// Gets performance recommendations
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        if self.network_utilization > 0.8 {
            recommendations.push("Consider reducing update frequencies or enabling more aggressive compression".to_string());
        }
        
        if self.updates_dropped > 0 {
            recommendations.push("Updates are being dropped - increase bandwidth limits or reduce object count".to_string());
        }
        
        if self.total_objects > 5000 {
            recommendations.push("High object count - consider spatial partitioning or object pooling".to_string());
        }
        
        if self.avg_batch_size < 5.0 {
            recommendations.push("Low batch efficiency - consider increasing batch size limits".to_string());
        }
        
        recommendations
    }
}

/// Example implementations for common game objects
pub mod examples {
    use super::*;
    use crate::types::Vec3;
    use serde::{Serialize, Deserialize};
    use std::any::Any;
    
    /// Example asteroid implementation
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ExampleAsteroid {
        pub position: Vec3,
        pub velocity: Vec3,
        pub health: f32,
        pub radius: f32,
        pub mineral_type: MineralType,
        pub rotation_speed: f32,
    }
    
    impl ExampleAsteroid {
        pub fn new(position: Vec3, mineral_type: MineralType) -> Self {
            Self {
                position,
                velocity: Vec3::new(0.0, 0.0, 0.0),
                health: 100.0,
                radius: 10.0,
                mineral_type,
                rotation_speed: 1.0,
            }
        }
    }
    
    impl GorcObject for ExampleAsteroid {
        fn type_name(&self) -> &'static str { "ExampleAsteroid" }
        
        fn position(&self) -> Vec3 { self.position }
        
        fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
            let distance = self.position.distance(observer_pos);
            if distance < 100.0 { ReplicationPriority::Critical }
            else if distance < 300.0 { ReplicationPriority::High }
            else if distance < 1000.0 { ReplicationPriority::Normal }
            else { ReplicationPriority::Low }
        }
        
        fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
            let mut data = serde_json::Map::new();
            
            for property in &layer.properties {
                match property.as_str() {
                    "position" => {
                        data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                    }
                    "velocity" => {
                        data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                    }
                    "health" => {
                        data.insert("health".to_string(), serde_json::to_value(self.health)?);
                    }
                    "radius" => {
                        data.insert("radius".to_string(), serde_json::to_value(self.radius)?);
                    }
                    "mineral_type" => {
                        data.insert("mineral_type".to_string(), serde_json::to_value(&self.mineral_type)?);
                    }
                    "rotation_speed" => {
                        data.insert("rotation_speed".to_string(), serde_json::to_value(self.rotation_speed)?);
                    }
                    _ => {}
                }
            }
            
            Ok(serde_json::to_vec(&data)?)
        }
        
        fn get_layers(&self) -> Vec<ReplicationLayer> {
            vec![
                // Critical: Position and collision data
                ReplicationLayer::new(
                    0, 100.0, 30.0,
                    vec!["position".to_string(), "velocity".to_string(), "health".to_string()],
                    CompressionType::Delta
                ),
                // Detailed: Visual state
                ReplicationLayer::new(
                    1, 300.0, 15.0,
                    vec!["rotation_speed".to_string()],
                    CompressionType::Lz4
                ),
                // Metadata: Strategic information
                ReplicationLayer::new(
                    3, 2000.0, 2.0,
                    vec!["mineral_type".to_string(), "radius".to_string()],
                    CompressionType::High
                ),
            ]
        }
        
        fn update_position(&mut self, new_position: Vec3) {
            self.position = new_position;
        }
        
        fn as_any(&self) -> &dyn Any { self }
        fn as_any_mut(&mut self) -> &mut dyn Any { self }
        
        fn clone_object(&self) -> Box<dyn GorcObject> {
            Box::new(self.clone())
        }
    }
    
    /// Example player implementation
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ExamplePlayer {
        pub position: Vec3,
        pub velocity: Vec3,
        pub health: f32,
        pub name: String,
        pub level: u32,
        pub equipment: Vec<String>,
    }
    
    impl ExamplePlayer {
        pub fn new(name: String, position: Vec3) -> Self {
            Self {
                position,
                velocity: Vec3::new(0.0, 0.0, 0.0),
                health: 100.0,
                name,
                level: 1,
                equipment: Vec::new(),
            }
        }
    }
    
    impl GorcObject for ExamplePlayer {
        fn type_name(&self) -> &'static str { "ExamplePlayer" }
        
        fn position(&self) -> Vec3 { self.position }
        
        fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
            let distance = self.position.distance(observer_pos);
            if distance < 50.0 { ReplicationPriority::Critical }
            else if distance < 200.0 { ReplicationPriority::High }
            else if distance < 500.0 { ReplicationPriority::Normal }
            else { ReplicationPriority::Low }
        }
        
        fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
            let mut data = serde_json::Map::new();
            
            for property in &layer.properties {
                match property.as_str() {
                    "position" => {
                        data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                    }
                    "velocity" => {
                        data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                    }
                    "health" => {
                        data.insert("health".to_string(), serde_json::to_value(self.health)?);
                    }
                    "name" => {
                        data.insert("name".to_string(), serde_json::to_value(&self.name)?);
                    }
                    "level" => {
                        data.insert("level".to_string(), serde_json::to_value(self.level)?);
                    }
                    "equipment" => {
                        data.insert("equipment".to_string(), serde_json::to_value(&self.equipment)?);
                    }
                    _ => {}
                }
            }
            
            Ok(serde_json::to_vec(&data)?)
        }
        
        fn get_layers(&self) -> Vec<ReplicationLayer> {
            vec![
                // Critical: Essential player state
                ReplicationLayer::new(
                    0, 50.0, 60.0,
                    vec!["position".to_string(), "velocity".to_string(), "health".to_string()],
                    CompressionType::Delta
                ),
                // Detailed: Equipment and visual state
                ReplicationLayer::new(
                    1, 200.0, 20.0,
                    vec!["equipment".to_string()],
                    CompressionType::Lz4
                ),
                // Metadata: Player information
                ReplicationLayer::new(
                    3, 1000.0, 5.0,
                    vec!["name".to_string(), "level".to_string()],
                    CompressionType::High
                ),
            ]
        }
        
        fn update_position(&mut self, new_position: Vec3) {
            self.position = new_position;
        }
        
        fn as_any(&self) -> &dyn Any { self }
        fn as_any_mut(&mut self) -> &mut dyn Any { self }
        
        fn clone_object(&self) -> Box<dyn GorcObject> {
            Box::new(self.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PlayerId, Vec3};
    
    #[tokio::test]
    async fn test_complete_gorc_system() {
        // This would require a mock ServerContext implementation
        // let server_context = Arc::new(MockServerContext::new());
        // let system = utils::create_complete_gorc_system(server_context).unwrap();
        
        // For now, just test that the types compile
        let instance_manager = Arc::new(GorcInstanceManager::new());
        let stats = instance_manager.get_stats().await;
        assert_eq!(stats.total_objects, 0);
    }
    
    #[test]
    fn test_example_asteroid() {
        let asteroid = examples::ExampleAsteroid::new(
            Vec3::new(100.0, 0.0, 200.0),
            MineralType::Platinum
        );
        
        assert_eq!(asteroid.type_name(), "ExampleAsteroid");
        assert_eq!(asteroid.position(), Vec3::new(100.0, 0.0, 200.0));
        
        let layers = asteroid.get_layers();
        assert!(layers.len() >= 2);
        
        // Test serialization
        if let Some(layer) = layers.first() {
            let serialized = asteroid.serialize_for_layer(layer);
            assert!(serialized.is_ok());
        }
    }
    
    #[test]
    fn test_performance_report() {
        let report = GorcPerformanceReport {
            timestamp: 123456789,
            total_objects: 100,
            total_subscriptions: 500,
            network_utilization: 0.3,
            events_sent: 10000,
            bytes_transmitted: 1024 * 1024,
            updates_dropped: 0,
            avg_batch_size: 15.0,
            issues: Vec::new(),
        };
        
        assert!(report.is_healthy());
        assert!(report.health_score() > 0.8);
        assert!(report.get_recommendations().is_empty());
    }
    
    #[test]
    fn test_performance_report_with_issues() {
        let report = GorcPerformanceReport {
            timestamp: 123456789,
            total_objects: 100,
            total_subscriptions: 500,
            network_utilization: 0.9, // High utilization
            events_sent: 10000,
            bytes_transmitted: 1024 * 1024,
            updates_dropped: 5, // Some drops
            avg_batch_size: 15.0,
            issues: vec!["Test issue".to_string()],
        };
        
        assert!(!report.is_healthy());
        assert!(report.health_score() < 0.5);
        assert!(!report.get_recommendations().is_empty());
    }
}