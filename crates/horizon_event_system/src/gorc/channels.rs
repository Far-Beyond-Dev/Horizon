//! # Replication Channels and Layer Management
//!
//! This module defines the core structures for GORC replication channels,
//! including channel configuration, replication layers, and the main GORC manager.

use crate::types::{Position, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// Compression algorithms available for replication data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionType {
    /// No compression - fastest but largest payload
    None,
    /// LZ4 compression - good balance of speed and size
    Lz4,
    /// Zlib compression - smaller payload but slower
    Zlib,
    /// Delta compression - only send changes from previous state
    Delta,
    /// Quantized compression - reduce precision for smaller payload
    Quantized,
    /// High compression - maximum compression for low-priority data
    High,
    /// Custom game-specific compression
    Custom(u8),
}

/// Priority levels for replication data
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ReplicationPriority {
    /// Critical data that must be delivered immediately
    Critical = 0,
    /// Important data with high priority
    High = 1,
    /// Normal priority data
    Normal = 2,
    /// Low priority data that can be delayed
    Low = 3,
}

/// Configuration for a replication layer within a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLayer {
    /// Channel number (0-3)
    pub channel: u8,
    /// Maximum transmission radius for this layer
    pub radius: f32,
    /// Target frequency in Hz
    pub frequency: f32,
    /// Properties to replicate at this layer
    pub properties: Vec<String>,
    /// Compression type for this layer
    pub compression: CompressionType,
    /// Priority level for this layer
    pub priority: ReplicationPriority,
}

impl ReplicationLayer {
    /// Creates a new replication layer
    pub fn new(
        channel: u8,
        radius: f32,
        frequency: f32,
        properties: Vec<String>,
        compression: CompressionType,
    ) -> Self {
        let priority = match channel {
            0 => ReplicationPriority::Critical,
            1 => ReplicationPriority::High,
            2 => ReplicationPriority::Normal,
            3 => ReplicationPriority::Low,
            _ => ReplicationPriority::Low,
        };

        Self {
            channel,
            radius,
            frequency,
            properties,
            compression,
            priority,
        }
    }

    /// Get the update interval for this layer
    pub fn update_interval(&self) -> Duration {
        Duration::from_millis((1000.0 / self.frequency) as u64)
    }
}

/// Replication channel configuration and state
#[derive(Debug, Clone)]
pub struct ReplicationChannel {
    /// Channel number (0-3)
    pub id: u8,
    /// Name of the channel
    pub name: String,
    /// Description of the channel's purpose
    pub description: String,
    /// Target frequency range for this channel
    pub frequency_range: (f32, f32),
    /// Layers configured for this channel
    pub layers: Vec<ReplicationLayer>,
    /// Last update timestamp
    pub last_update: Option<Instant>,
    /// Statistics for this channel
    pub stats: ChannelStats,
}

impl ReplicationChannel {
    /// Creates a new replication channel
    pub fn new(id: u8, name: String, description: String, frequency_range: (f32, f32)) -> Self {
        Self {
            id,
            name,
            description,
            frequency_range,
            layers: Vec::new(),
            last_update: None,
            stats: ChannelStats::default(),
        }
    }

    /// Adds a replication layer to this channel
    pub fn add_layer(&mut self, layer: ReplicationLayer) {
        if layer.channel == self.id {
            self.layers.push(layer);
        }
    }

    /// Checks if the channel is ready for update based on its frequency
    pub fn is_ready_for_update(&self) -> bool {
        match self.last_update {
            None => true,
            Some(last) => {
                let min_interval = Duration::from_millis((1000.0 / self.frequency_range.1) as u64);
                last.elapsed() >= min_interval
            }
        }
    }

    /// Marks the channel as updated
    pub fn mark_updated(&mut self) {
        self.last_update = Some(Instant::now());
        self.stats.updates_sent += 1;
    }
}

/// Statistics for a replication channel
#[derive(Debug, Default, Clone)]
pub struct ChannelStats {
    /// Number of updates sent through this channel
    pub updates_sent: u64,
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    /// Number of subscribers
    pub subscriber_count: usize,
    /// Average update frequency achieved
    pub avg_frequency: f32,
}

/// Main GORC manager that orchestrates all replication channels
#[derive(Debug)]
pub struct GorcManager {
    /// All configured replication channels
    channels: Arc<RwLock<HashMap<u8, ReplicationChannel>>>,
    /// Cached replication layers for quick access
    layers: Arc<RwLock<HashMap<String, ReplicationLayer>>>,
    /// Global GORC statistics
    stats: Arc<RwLock<GorcStats>>,
}

impl GorcManager {
    /// Creates a new GORC manager with default channels
    pub fn new() -> Self {
        let mut manager = Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            layers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(GorcStats::default())),
        };

        // Initialize default channels
        manager.initialize_default_channels();
        manager
    }

    // TODO: This will be deprecated in favor of the dynamic channel system
    /// Initializes the four default GORC channels
    fn initialize_default_channels(&mut self) {
        let _default_channels: Vec<(i32, &'static str, &'static str, (f64, f64))> = vec![
            (
                0,
                "Critical",
                "Essential game state (position, health, collision)",
                (30.0, 60.0),
            ),
            (
                1,
                "Detailed",
                "Important non-critical info (animations, weapons, interactions)",
                (15.0, 30.0),
            ),
            (
                2,
                "Cosmetic",
                "Visual enhancements (particles, effects)",
                (5.0, 15.0),
            ),
            (
                3,
                "Metadata",
                "Informational data (player names, achievements)",
                (1.0, 5.0),
            ),
        ];

        // Note: In a real implementation, we would use tokio::spawn or similar
        // to initialize channels asynchronously. For now, we'll create a blocking
        // initialization method.
    }

    /// Gets a reference to a specific channel
    pub async fn get_channel(&self, channel_id: u8) -> Option<ReplicationChannel> {
        let channels = self.channels.read().await;
        channels.get(&channel_id).cloned()
    }

    /// Adds a replication layer to the system
    pub async fn add_layer(&self, layer_name: String, layer: ReplicationLayer) {
        let mut layers = self.layers.write().await;
        layers.insert(layer_name, layer);
    }

    /// Gets the replication priority for an object at a given observer position
    pub async fn get_priority(&self, object_pos: Position, observer_pos: Position) -> ReplicationPriority {
        let distance = self.calculate_distance(object_pos, observer_pos);
        
        // Priority based on distance
        if distance < 50.0 {
            ReplicationPriority::Critical
        } else if distance < 150.0 {
            ReplicationPriority::High
        } else if distance < 500.0 {
            ReplicationPriority::Normal
        } else {
            ReplicationPriority::Low
        }
    }

    /// Calculates distance between two positions
    fn calculate_distance(&self, pos1: Position, pos2: Position) -> f32 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
    }

    /// Gets current GORC statistics
    pub async fn get_stats(&self) -> GorcStats {
        self.stats.read().await.clone()
    }
}

impl Default for GorcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global GORC system statistics
#[derive(Debug, Clone, Default)]
pub struct GorcStats {
    /// Total number of active subscriptions
    pub total_subscriptions: usize,
    /// Total bytes transmitted across all channels
    pub total_bytes_transmitted: u64,
    /// Average update frequency across all channels
    pub avg_update_frequency: f32,
    /// Number of multicast groups
    pub multicast_groups: usize,
    /// Memory usage in bytes
    pub memory_usage: usize,
}

/// Builder for replication layers with fluent API
#[derive(Debug, Clone, Default)]
pub struct ReplicationLayers {
    /// The layers configured for this object type
    layers: Vec<ReplicationLayer>,
}

impl ReplicationLayers {
    /// Creates a new empty ReplicationLayers builder
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
        }
    }

    /// Adds a replication layer to the builder
    /// 
    /// # Arguments
    /// 
    /// * `layer` - The replication layer to add
    /// 
    /// # Returns
    /// 
    /// Returns self for method chaining
    pub fn add_layer(mut self, layer: ReplicationLayer) -> Self {
        self.layers.push(layer);
        self
    }

    /// Gets all layers as a vector
    pub fn into_layers(self) -> Vec<ReplicationLayer> {
        self.layers
    }

    /// Gets a reference to all layers
    pub fn layers(&self) -> &[ReplicationLayer] {
        &self.layers
    }

    /// Gets the number of layers
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// Returns true if there are no layers
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Mineral types for game objects like asteroids
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MineralType {
    Iron,
    Copper,
    Gold,
    Platinum,
    Uranium,
    Rare(String),
}

impl Default for MineralType {
    fn default() -> Self {
        MineralType::Iron
    }
}

/// Trait for objects that can be replicated through GORC
pub trait Replication {
    /// Initialize replication layers for this object type
    fn init_layers() -> ReplicationLayers;
    
    /// Get the replication priority for this object at the observer's position
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority;
    
    /// Serialize data for a specific replication layer
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
}

/// Registry for managing registered GORC objects
#[derive(Debug)]
pub struct GorcObjectRegistry {
    /// Map of object type names to their replication layers
    registered_objects: Arc<RwLock<HashMap<String, Vec<ReplicationLayer>>>>,
    /// Statistics about registered objects
    stats: Arc<RwLock<RegistryStats>>,
}

impl GorcObjectRegistry {
    /// Creates a new object registry
    pub fn new() -> Self {
        Self {
            registered_objects: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RegistryStats::default())),
        }
    }

    /// Registers an object type with its replication layers
    pub async fn register_object<T: Replication + 'static>(&self, object_name: String) {
        let layers = T::init_layers().into_layers();
        
        {
            let mut objects = self.registered_objects.write().await;
            objects.insert(object_name.clone(), layers);
        }

        {
            let mut stats = self.stats.write().await;
            stats.registered_objects += 1;
        }

        tracing::info!("📦 Registered GORC object type: {}", object_name);
    }

    /// Gets the replication layers for a registered object type
    pub async fn get_layers(&self, object_name: &str) -> Option<Vec<ReplicationLayer>> {
        let objects = self.registered_objects.read().await;
        objects.get(object_name).cloned()
    }

    /// Lists all registered object types
    pub async fn list_objects(&self) -> Vec<String> {
        let objects = self.registered_objects.read().await;
        objects.keys().cloned().collect()
    }

    /// Gets registry statistics
    pub async fn get_stats(&self) -> RegistryStats {
        self.stats.read().await.clone()
    }
}

impl Default for GorcObjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for the object registry
#[derive(Debug, Clone, Default)]
pub struct RegistryStats {
    /// Number of registered object types
    pub registered_objects: usize,
    /// Total number of replication layers across all objects
    pub total_layers: usize,
    /// Average layers per object
    pub avg_layers_per_object: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_layer_creation() {
        let layer = ReplicationLayer::new(
            0,
            100.0,
            60.0,
            vec!["position".to_string(), "health".to_string()],
            CompressionType::Lz4,
        );

        assert_eq!(layer.channel, 0);
        assert_eq!(layer.radius, 100.0);
        assert_eq!(layer.frequency, 60.0);
        assert_eq!(layer.priority, ReplicationPriority::Critical);
        assert_eq!(layer.properties.len(), 2);
    }

    #[test]
    fn test_channel_update_timing() {
        let mut channel = ReplicationChannel::new(
            0,
            "Test".to_string(),
            "Test channel".to_string(),
            (30.0, 60.0),
        );

        assert!(channel.is_ready_for_update());
        
        channel.mark_updated();
        assert_eq!(channel.stats.updates_sent, 1);
    }

    #[tokio::test]
    async fn test_gorc_manager_creation() {
        let manager = GorcManager::new();
        let stats = manager.get_stats().await;
        
        // Initial state should have default values
        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.total_bytes_transmitted, 0);
    }

    #[tokio::test]
    async fn test_priority_calculation() {
        let manager = GorcManager::new();
        let pos1 = Position::new(0.0, 0.0, 0.0);
        let pos2 = Position::new(25.0, 0.0, 0.0);
        
        let priority = manager.get_priority(pos1, pos2).await;
        assert_eq!(priority, ReplicationPriority::Critical);
    }
}