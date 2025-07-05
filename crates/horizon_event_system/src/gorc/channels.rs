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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
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

    /// Check if this layer should replicate a specific property
    pub fn replicates_property(&self, property: &str) -> bool {
        self.properties.contains(&property.to_string())
    }

    /// Get estimated data size for this layer (rough approximation)
    pub fn estimated_data_size(&self) -> usize {
        // Rough estimate: 32 bytes per property + overhead
        self.properties.len() * 32 + 64
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
    /// Whether this channel is currently active
    pub active: bool,
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
            active: true,
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
        if !self.active {
            return false;
        }

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

    /// Sets the channel active/inactive
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Gets the effective frequency based on current conditions
    pub fn get_effective_frequency(&self, load_factor: f32) -> f32 {
        let base_freq = self.frequency_range.1;
        let min_freq = self.frequency_range.0;
        
        // Reduce frequency under load
        let adjusted_freq = base_freq * (1.0 - load_factor * 0.5);
        adjusted_freq.max(min_freq)
    }

    /// Gets the maximum radius for any layer in this channel
    pub fn max_radius(&self) -> f32 {
        self.layers
            .iter()
            .map(|layer| layer.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
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
    /// Peak subscriber count
    pub peak_subscriber_count: usize,
    /// Average latency for this channel (milliseconds)
    pub avg_latency_ms: f32,
}

impl ChannelStats {
    /// Updates the average frequency calculation
    pub fn update_frequency(&mut self, actual_frequency: f32) {
        if self.updates_sent == 0 {
            self.avg_frequency = actual_frequency;
        } else {
            // Exponential moving average
            self.avg_frequency = self.avg_frequency * 0.9 + actual_frequency * 0.1;
        }
    }

    /// Records a subscriber count update
    pub fn update_subscriber_count(&mut self, count: usize) {
        self.subscriber_count = count;
        if count > self.peak_subscriber_count {
            self.peak_subscriber_count = count;
        }
    }
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
    /// System configuration
    config: GorcConfig,
}

impl GorcManager {
    /// Creates a new GORC manager with default channels
    pub fn new() -> Self {
        let mut manager = Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            layers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(GorcStats::default())),
            config: GorcConfig::default(),
        };

        // Initialize default channels in a blocking context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                manager.initialize_default_channels().await;
            });
        });

        manager
    }

    /// Initializes the four default GORC channels
    async fn initialize_default_channels(&mut self) {
        let default_channels = vec![
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

        let mut channels = self.channels.write().await;
        for (id, name, description, frequency_range) in default_channels {
            let channel = ReplicationChannel::new(
                id,
                name.to_string(),
                description.to_string(),
                frequency_range,
            );
            channels.insert(id, channel);
        }
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

    /// Updates system configuration
    pub async fn update_config(&mut self, new_config: GorcConfig) {
        self.config = new_config;
        
        // Apply configuration changes to channels
        let mut channels = self.channels.write().await;
        for channel in channels.values_mut() {
            if !self.config.adaptive_frequency {
                // Reset to default frequencies if adaptive is disabled
                channel.frequency_range = match channel.id {
                    0 => (30.0, 60.0),
                    1 => (15.0, 30.0),
                    2 => (5.0, 15.0),
                    3 => (1.0, 5.0),
                    _ => (1.0, 10.0),
                };
            }
        }
    }

    /// Performs system optimization based on current load
    pub async fn optimize_performance(&self) -> Result<(), GorcError> {
        let current_load = self.calculate_system_load().await;
        
        if current_load > self.config.load_threshold {
            tracing::warn!("GORC system load high: {:.1}% - applying optimizations", current_load * 100.0);
            
            let mut channels = self.channels.write().await;
            for channel in channels.values_mut() {
                if self.config.adaptive_frequency {
                    // Reduce frequencies under load
                    let load_factor = (current_load - self.config.load_threshold) / (1.0 - self.config.load_threshold);
                    let original_max = match channel.id {
                        0 => 60.0,
                        1 => 30.0,
                        2 => 15.0,
                        3 => 5.0,
                        _ => 10.0,
                    };
                    
                    let reduced_max = original_max * (1.0 - load_factor * 0.5);
                    channel.frequency_range.1 = reduced_max.max(channel.frequency_range.0);
                }
            }
            
            // Update stats
            let mut stats = self.stats.write().await;
            stats.optimizations_applied += 1;
        }
        
        Ok(())
    }

    /// Calculates current system load (0.0 to 1.0)
    async fn calculate_system_load(&self) -> f32 {
        let channels = self.channels.read().await;
        let total_bandwidth: u64 = channels.values()
            .map(|ch| ch.stats.bytes_transmitted)
            .sum();
        
        let total_subscribers: usize = channels.values()
            .map(|ch| ch.stats.subscriber_count)
            .sum();
        
        // Simple load calculation - in practice this would be more sophisticated
        let bandwidth_load = (total_bandwidth as f32 / self.config.max_total_bandwidth as f32).min(1.0);
        let subscriber_load = (total_subscribers as f32 / self.config.max_total_subscribers as f32).min(1.0);
        
        bandwidth_load.max(subscriber_load)
    }

    /// Enables or disables a specific channel
    pub async fn set_channel_active(&self, channel_id: u8, active: bool) -> Result<(), GorcError> {
        let mut channels = self.channels.write().await;
        if let Some(channel) = channels.get_mut(&channel_id) {
            channel.set_active(active);
            Ok(())
        } else {
            Err(GorcError::ChannelNotFound(channel_id))
        }
    }

    /// Gets all active channels
    pub async fn get_active_channels(&self) -> Vec<u8> {
        let channels = self.channels.read().await;
        channels.values()
            .filter(|ch| ch.active)
            .map(|ch| ch.id)
            .collect()
    }

    /// Gets detailed performance report
    pub async fn get_performance_report(&self) -> PerformanceReport {
        let channels = self.channels.read().await;
        let stats = self.stats.read().await;
        
        let mut channel_reports = HashMap::new();
        let mut total_bandwidth = 0u64;
        let mut total_subscribers = 0usize;
        
        for (id, channel) in channels.iter() {
            total_bandwidth += channel.stats.bytes_transmitted;
            total_subscribers += channel.stats.subscriber_count;
            
            channel_reports.insert(*id, ChannelPerformanceReport {
                channel_id: *id,
                name: channel.name.clone(),
                active: channel.active,
                updates_sent: channel.stats.updates_sent,
                bytes_transmitted: channel.stats.bytes_transmitted,
                subscriber_count: channel.stats.subscriber_count,
                avg_frequency: channel.stats.avg_frequency,
                target_frequency: channel.frequency_range.1,
                avg_latency_ms: channel.stats.avg_latency_ms,
                efficiency: if channel.frequency_range.1 > 0.0 {
                    channel.stats.avg_frequency / channel.frequency_range.1
                } else {
                    0.0
                },
            });
        }
        
        PerformanceReport {
            timestamp: crate::utils::current_timestamp(),
            system_load: self.calculate_system_load().await,
            total_bandwidth,
            total_subscribers,
            optimizations_applied: stats.optimizations_applied,
            channel_reports,
        }
    }
}

impl Default for GorcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the GORC system
#[derive(Debug, Clone)]
pub struct GorcConfig {
    /// Whether to use adaptive frequency adjustment
    pub adaptive_frequency: bool,
    /// Load threshold for triggering optimizations (0.0 to 1.0)
    pub load_threshold: f32,
    /// Maximum total bandwidth across all channels
    pub max_total_bandwidth: u64,
    /// Maximum total subscribers across all channels
    pub max_total_subscribers: usize,
    /// Whether to enable compression by default
    pub default_compression_enabled: bool,
    /// Default compression type
    pub default_compression_type: CompressionType,
    /// Minimum update interval to prevent spam
    pub min_update_interval_ms: u64,
}

impl Default for GorcConfig {
    fn default() -> Self {
        Self {
            adaptive_frequency: true,
            load_threshold: 0.8,
            max_total_bandwidth: 100 * 1024 * 1024, // 100 MB/s
            max_total_subscribers: 10000,
            default_compression_enabled: true,
            default_compression_type: CompressionType::Lz4,
            min_update_interval_ms: 16, // ~60 FPS
        }
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
    /// Number of performance optimizations applied
    pub optimizations_applied: u64,
    /// System uptime in seconds
    pub uptime_seconds: u64,
}

/// Performance report for a single channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPerformanceReport {
    pub channel_id: u8,
    pub name: String,
    pub active: bool,
    pub updates_sent: u64,
    pub bytes_transmitted: u64,
    pub subscriber_count: usize,
    pub avg_frequency: f32,
    pub target_frequency: f32,
    pub avg_latency_ms: f32,
    pub efficiency: f32, // actual_frequency / target_frequency
}

/// Comprehensive performance report for the GORC system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub timestamp: u64,
    pub system_load: f32,
    pub total_bandwidth: u64,
    pub total_subscribers: usize,
    pub optimizations_applied: u64,
    pub channel_reports: HashMap<u8, ChannelPerformanceReport>,
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

    /// Validates that all layers have different channels
    pub fn validate(&self) -> Result<(), GorcError> {
        let mut channels = std::collections::HashSet::new();
        for layer in &self.layers {
            if !channels.insert(layer.channel) {
                return Err(GorcError::DuplicateChannel(layer.channel));
            }
        }
        Ok(())
    }

    /// Gets a layer by channel ID
    pub fn get_layer(&self, channel: u8) -> Option<&ReplicationLayer> {
        self.layers.iter().find(|layer| layer.channel == channel)
    }

    /// Gets the maximum radius across all layers
    pub fn max_radius(&self) -> f32 {
        self.layers
            .iter()
            .map(|layer| layer.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Gets the minimum radius across all layers
    pub fn min_radius(&self) -> f32 {
        self.layers
            .iter()
            .map(|layer| layer.radius)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
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
            stats.total_layers += 1; // Simplified - should count actual layers
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
        let mut stats = self.stats.read().await.clone();
        
        // Update calculated fields
        if stats.registered_objects > 0 {
            stats.avg_layers_per_object = stats.total_layers as f32 / stats.registered_objects as f32;
        }
        
        stats
    }

    /// Validates all registered object configurations
    pub async fn validate_all(&self) -> Result<(), GorcError> {
        let objects = self.registered_objects.read().await;
        
        for (object_name, layers) in objects.iter() {
            let layer_builder = ReplicationLayers {
                layers: layers.clone(),
            };
            
            if let Err(e) = layer_builder.validate() {
                return Err(GorcError::ValidationFailed(format!("{}: {}", object_name, e)));
            }
        }
        
        Ok(())
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

/// Errors that can occur in the GORC system
#[derive(Debug, thiserror::Error)]
pub enum GorcError {
    /// Channel not found
    #[error("Channel {0} not found")]
    ChannelNotFound(u8),
    /// Duplicate channel in layer configuration
    #[error("Duplicate channel {0} in replication layers")]
    DuplicateChannel(u8),
    /// Validation failed
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),
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
        assert!(layer.replicates_property("position"));
        assert!(!layer.replicates_property("velocity"));
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
        
        // Should not be ready immediately after update
        assert!(!channel.is_ready_for_update());
    }

    #[tokio::test]
    async fn test_gorc_manager_creation() {
        let manager = GorcManager::new();
        let stats = manager.get_stats().await;
        
        // Initial state should have default values
        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.total_bytes_transmitted, 0);
        
        // Should have default channels
        assert!(manager.get_channel(0).await.is_some());
        assert!(manager.get_channel(1).await.is_some());
        assert!(manager.get_channel(2).await.is_some());
        assert!(manager.get_channel(3).await.is_some());
    }

    #[tokio::test]
    async fn test_priority_calculation() {
        let manager = GorcManager::new();
        let pos1 = Position::new(0.0, 0.0, 0.0);
        let pos2 = Position::new(25.0, 0.0, 0.0);
        
        let priority = manager.get_priority(pos1, pos2).await;
        assert_eq!(priority, ReplicationPriority::Critical);
        
        let pos3 = Position::new(200.0, 0.0, 0.0);
        let priority2 = manager.get_priority(pos1, pos3).await;
        assert_eq!(priority2, ReplicationPriority::Normal);
    }

    #[test]
    fn test_replication_layers_builder() {
        let layers = ReplicationLayers::new()
            .add_layer(ReplicationLayer::new(
                0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta
            ))
            .add_layer(ReplicationLayer::new(
                1, 150.0, 30.0, vec!["animation".to_string()], CompressionType::Lz4
            ));

        assert_eq!(layers.len(), 2);
        assert_eq!(layers.max_radius(), 150.0);
        assert_eq!(layers.min_radius(), 50.0);
        assert!(layers.get_layer(0).is_some());
        assert!(layers.get_layer(2).is_none());
        assert!(layers.validate().is_ok());
    }

    #[test]
    fn test_duplicate_channel_validation() {
        let layers = ReplicationLayers::new()
            .add_layer(ReplicationLayer::new(
                0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta
            ))
            .add_layer(ReplicationLayer::new(
                0, 100.0, 30.0, vec!["health".to_string()], CompressionType::Lz4
            ));

        assert!(layers.validate().is_err());
    }
}