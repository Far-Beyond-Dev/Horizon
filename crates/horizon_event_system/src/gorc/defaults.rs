//! Default configurations for the GORC system.
//!
//! This module provides sensible default configurations for various GORC components,
//! optimized for typical game server scenarios with balanced performance characteristics.

use super::{
    CompressionType, GorcConfig, NetworkConfig, ReplicationLayer, ReplicationLayers,
    ReplicationPriority, ZoneConfig,
};
use std::collections::HashMap;

/// Creates default replication layers for a typical game object.
/// 
/// These layers provide a good starting point for most game objects with
/// appropriate distance thresholds and update frequencies for different
/// types of data.
/// 
/// # Layer Configuration
/// 
/// * **Channel 0 (Critical)**: Position and health data at 30Hz within 50 units
/// * **Channel 1 (Detailed)**: Animation and state at 15Hz within 150 units
/// * **Channel 2 (Cosmetic)**: Effects at 10Hz within 300 units
/// * **Channel 3 (Metadata)**: Name and metadata at 2Hz within 1000 units
/// 
/// # Returns
/// 
/// A configured `ReplicationLayers` instance with four standard layers.
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

/// Creates default network configuration optimized for most games.
/// 
/// This configuration balances bandwidth usage with responsiveness,
/// providing good performance for typical multiplayer scenarios.
/// 
/// # Configuration Details
/// 
/// * **Bandwidth**: 512 KB/s per player maximum
/// * **Batching**: Maximum 25 updates per batch, 16ms age limit (~60 FPS)
/// * **Frequencies**: Tiered update rates from 30Hz (critical) to 2Hz (metadata)
/// * **Compression**: Enabled with 128-byte threshold
/// * **Priority Queues**: Sized based on importance level
/// 
/// # Returns
/// 
/// A configured `NetworkConfig` instance ready for production use.
pub fn default_network_config() -> NetworkConfig {
    NetworkConfig {
        max_bandwidth_per_player: 512 * 1024, // 512 KB/s per player
        max_batch_size: 25,
        max_batch_age_ms: 16, // ~60 FPS
        target_frequencies: {
            let mut freq = HashMap::new();
            freq.insert(0, 30.0); // Critical - 30Hz
            freq.insert(1, 15.0); // Detailed - 15Hz
            freq.insert(2, 10.0); // Cosmetic - 10Hz
            freq.insert(3, 2.0);  // Metadata - 2Hz
            freq
        },
        compression_enabled: true,
        compression_threshold: 128,
        priority_queue_sizes: {
            let mut sizes = HashMap::new();
            sizes.insert(ReplicationPriority::Critical, 500);
            sizes.insert(ReplicationPriority::High, 250);
            sizes.insert(ReplicationPriority::Normal, 100);
            sizes.insert(ReplicationPriority::Low, 50);
            sizes
        },
    }
}

/// Creates default zone configuration for balanced performance.
/// 
/// This configuration provides stable zone management with adaptive sizing
/// and reasonable subscriber limits to maintain good performance.
/// 
/// # Configuration Details
/// 
/// * **Hysteresis**: 10% factor to prevent subscription flapping
/// * **Update Rate**: Minimum 30 FPS for zone updates
/// * **Subscribers**: Maximum 50 per zone to maintain performance
/// * **Adaptive Sizing**: Enabled with 20% scale factor
/// 
/// # Returns
/// 
/// A configured `ZoneConfig` instance optimized for stability.
pub fn default_zone_config() -> ZoneConfig {
    ZoneConfig {
        hysteresis_factor: 0.1, // 10% hysteresis to prevent flapping
        min_update_interval_ms: 33, // ~30 FPS minimum
        max_subscribers_per_zone: 50,
        adaptive_sizing: true,
        adaptive_scale_factor: 0.2,
    }
}

/// Creates a default GORC system configuration.
/// 
/// Combines all default configurations into a complete GORC setup
/// suitable for most game server deployments without additional tuning.
/// 
/// # Returns
/// 
/// A fully configured `GorcConfig` instance with production-ready defaults.
pub fn default_gorc_config() -> GorcConfig {
    GorcConfig {
        max_objects: 10000,
        default_compression: CompressionType::Lz4,
        adaptive_frequency: true,
        optimization_level: 2,
    }
}