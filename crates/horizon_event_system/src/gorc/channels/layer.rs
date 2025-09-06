/// Replication layer definitions and management
use super::types::{CompressionType, ReplicationPriority};
use crate::Vec3;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

/// Configuration for a replication layer within a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLayer {
    /// Channel number (0-3)
    pub channel: u8,
    /// Maximum transmission radius for this layer
    pub radius: f64,
    /// Target frequency in Hz
    pub frequency: f64,
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
        radius: f64,
        frequency: f64,
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

    /// Check if a position is within this layer's radius
    pub fn contains_position(&self, center: Vec3, position: Vec3) -> bool {
        center.distance(position) <= self.radius
    }

    /// Get the compression ratio estimate for this layer
    pub fn estimated_compression_ratio(&self) -> f32 {
        match self.compression {
            CompressionType::None => 1.0,
            CompressionType::Lz4 => 0.7,
            CompressionType::Zlib => 0.6,
            CompressionType::Delta => 0.3,
            CompressionType::Quantized => 0.5,
            CompressionType::High => 0.4,
            CompressionType::Custom(_) => 0.5,
        }
    }
}

/// Collection of replication layers with utility methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationLayers {
    pub layers: Vec<ReplicationLayer>,
}

impl ReplicationLayers {
    /// Creates a new empty collection of layers
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
        }
    }

    /// Adds a layer to the collection
    pub fn add_layer(&mut self, layer: ReplicationLayer) {
        self.layers.push(layer);
    }

    /// Gets layers for a specific channel
    pub fn get_layers_for_channel(&self, channel: u8) -> Vec<&ReplicationLayer> {
        self.layers.iter().filter(|l| l.channel == channel).collect()
    }

    /// Gets all layers that contain a specific position relative to a center
    pub fn get_layers_for_position(&self, center: Vec3, position: Vec3) -> Vec<&ReplicationLayer> {
        self.layers
            .iter()
            .filter(|l| l.contains_position(center, position))
            .collect()
    }

    /// Gets layers that replicate a specific property
    pub fn get_layers_for_property(&self, property: &str) -> Vec<&ReplicationLayer> {
        self.layers
            .iter()
            .filter(|l| l.replicates_property(property))
            .collect()
    }

    /// Gets the highest priority layer that replicates a property
    pub fn get_highest_priority_layer_for_property(&self, property: &str) -> Option<&ReplicationLayer> {
        self.get_layers_for_property(property)
            .into_iter()
            .min_by_key(|l| l.priority)
    }

    /// Optimizes layer configuration by removing redundant layers
    pub fn optimize(&mut self) {
        // Sort by priority first
        self.layers.sort_by_key(|l| l.priority);
        
        // Remove duplicate layers (simplified optimization)
        self.layers.dedup_by(|a, b| {
            a.channel == b.channel && 
            a.radius == b.radius && 
            a.properties == b.properties
        });
    }

    /// Gets total estimated bandwidth for all layers
    pub fn estimated_total_bandwidth(&self) -> f64 {
        self.layers
            .iter()
            .map(|l| l.estimated_data_size() as f64 * l.frequency)
            .sum()
    }

    /// Creates a default layer configuration for standard GORC usage
    pub fn create_default() -> Self {
        let mut layers = Self::new();
        
        // Critical channel - position and health
        layers.add_layer(ReplicationLayer::new(
            0,
            50.0,
            60.0,
            vec!["position".to_string(), "health".to_string()],
            CompressionType::Delta,
        ));
        
        // Detailed channel - velocity and detailed state
        layers.add_layer(ReplicationLayer::new(
            1,
            150.0,
            30.0,
            vec!["velocity".to_string(), "rotation".to_string(), "detailed_state".to_string()],
            CompressionType::Lz4,
        ));
        
        // Cosmetic channel - animations and effects
        layers.add_layer(ReplicationLayer::new(
            2,
            300.0,
            15.0,
            vec!["animation_state".to_string(), "effect_state".to_string()],
            CompressionType::Lz4,
        ));
        
        // Metadata channel - static properties
        layers.add_layer(ReplicationLayer::new(
            3,
            1000.0,
            5.0,
            vec!["object_type".to_string(), "metadata".to_string(), "tags".to_string()],
            CompressionType::High,
        ));
        
        layers
    }
}