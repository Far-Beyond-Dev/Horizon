//! GORC Configuration Management
//!
//! This module provides configuration structures for the entire GORC system,
//! including virtualization settings, performance tuning, and feature flags.

use crate::gorc::virtualization::VirtualizationConfig;
use serde::{Deserialize, Serialize};

/// Complete GORC system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcServerConfig {
    /// General GORC settings
    pub general: GorcGeneralConfig,
    /// Zone virtualization configuration
    pub virtualization: VirtualizationConfig,
    /// Spatial indexing configuration
    pub spatial: SpatialConfig,
    /// Network replication configuration
    pub network: NetworkConfig,
    /// Performance monitoring configuration
    pub monitoring: MonitoringConfig,
}

impl Default for GorcServerConfig {
    fn default() -> Self {
        Self {
            general: GorcGeneralConfig::default(),
            virtualization: VirtualizationConfig::default(),
            spatial: SpatialConfig::default(),
            network: NetworkConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

/// General GORC system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcGeneralConfig {
    /// Maximum number of objects that can be registered
    pub max_objects: usize,
    /// Maximum number of concurrent players
    pub max_players: usize,
    /// Maximum number of channels per object
    pub max_channels_per_object: u8,
    /// Enable automatic zone optimization
    pub auto_optimize_zones: bool,
    /// Frequency of zone optimization checks (in milliseconds)
    pub optimization_interval_ms: u64,
    /// Enable debug logging for GORC operations
    pub debug_logging: bool,
}

impl Default for GorcGeneralConfig {
    fn default() -> Self {
        Self {
            max_objects: 10000,
            max_players: 1000,
            max_channels_per_object: 8,
            auto_optimize_zones: true,
            optimization_interval_ms: 5000, // 5 seconds
            debug_logging: false,
        }
    }
}

/// Spatial indexing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialConfig {
    /// World bounds for spatial partitioning (min_x, min_y, min_z, max_x, max_y, max_z)
    pub world_bounds: (f64, f64, f64, f64, f64, f64),
    /// Maximum objects stored in a single R-tree leaf node
    pub max_objects_per_leaf: usize,
    /// Number of mutations before triggering a bulk rebuild
    pub rebuild_threshold: usize,
    /// Enable spatial index caching
    pub enable_caching: bool,
    /// Cache expiry time in milliseconds
    pub cache_expiry_ms: u64,
}

impl Default for SpatialConfig {
    fn default() -> Self {
        Self {
            world_bounds: (-10000.0, -10000.0, -1000.0, 10000.0, 10000.0, 1000.0),
            max_objects_per_leaf: 64,
            rebuild_threshold: 5_000,
            enable_caching: true,
            cache_expiry_ms: 30000, // 30 seconds
        }
    }
}

/// Network replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Maximum batch size for replication updates
    pub max_batch_size: usize,
    /// Update frequency for each channel (Hz)
    pub channel_frequencies: [f64; 4],
    /// Enable compression for replication data
    pub enable_compression: bool,
    /// Compression threshold in bytes
    pub compression_threshold: usize,
    /// Maximum queue size per player
    pub max_queue_size_per_player: usize,
    /// Network timeout in milliseconds
    pub network_timeout_ms: u64,
    /// Enable priority-based sending
    pub enable_priority_sending: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            channel_frequencies: [60.0, 30.0, 15.0, 5.0], // Hz for channels 0-3
            enable_compression: true,
            compression_threshold: 1024, // 1KB
            max_queue_size_per_player: 10000,
            network_timeout_ms: 5000, // 5 seconds
            enable_priority_sending: true,
        }
    }
}

/// Performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable performance statistics collection
    pub enable_stats: bool,
    /// Statistics reporting interval in milliseconds
    pub stats_interval_ms: u64,
    /// Enable performance profiling
    pub enable_profiling: bool,
    /// Maximum number of performance samples to keep
    pub max_performance_samples: usize,
    /// Enable memory usage tracking
    pub track_memory_usage: bool,
    /// Log slow operations (threshold in microseconds)
    pub slow_operation_threshold_us: u64,
    /// Enable real-time performance alerts
    pub enable_performance_alerts: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_stats: true,
            stats_interval_ms: 10000, // 10 seconds
            enable_profiling: false,
            max_performance_samples: 1000,
            track_memory_usage: true,
            slow_operation_threshold_us: 1000, // 1ms
            enable_performance_alerts: true,
        }
    }
}

/// Configuration builder for easier setup
pub struct GorcConfigBuilder {
    config: GorcServerConfig,
}

impl GorcConfigBuilder {
    /// Creates a new configuration builder with default values
    pub fn new() -> Self {
        Self {
            config: GorcServerConfig::default(),
        }
    }

    /// Enables zone virtualization with specified settings
    pub fn with_virtualization(mut self, enabled: bool, density_threshold: f64) -> Self {
        self.config.virtualization.enabled = enabled;
        self.config.virtualization.density_threshold = density_threshold;
        self
    }

    /// Sets world bounds for spatial partitioning
    pub fn with_world_bounds(mut self, min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Self {
        self.config.spatial.world_bounds = (min_x, min_y, min_z, max_x, max_y, max_z);
        self
    }

    /// Sets maximum number of objects and players
    pub fn with_capacity(mut self, max_objects: usize, max_players: usize) -> Self {
        self.config.general.max_objects = max_objects;
        self.config.general.max_players = max_players;
        self
    }

    /// Enables debug logging
    pub fn with_debug_logging(mut self, enabled: bool) -> Self {
        self.config.general.debug_logging = enabled;
        self
    }

    /// Sets network configuration
    pub fn with_network_config(mut self, max_batch_size: usize, enable_compression: bool) -> Self {
        self.config.network.max_batch_size = max_batch_size;
        self.config.network.enable_compression = enable_compression;
        self
    }

    /// Sets channel update frequencies
    pub fn with_channel_frequencies(mut self, frequencies: [f64; 4]) -> Self {
        self.config.network.channel_frequencies = frequencies;
        self
    }

    /// Enables performance monitoring
    pub fn with_monitoring(mut self, enable_stats: bool, enable_profiling: bool) -> Self {
        self.config.monitoring.enable_stats = enable_stats;
        self.config.monitoring.enable_profiling = enable_profiling;
        self
    }

    /// Builds the final configuration
    pub fn build(self) -> GorcServerConfig {
        self.config
    }
}

impl Default for GorcConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Preset configurations for common use cases
pub mod presets {
    use super::*;

    /// High-performance configuration for large-scale multiplayer games
    pub fn high_performance() -> GorcServerConfig {
        GorcConfigBuilder::new()
            .with_capacity(50000, 5000)
            .with_virtualization(true, 0.3)
            .with_world_bounds(-50000.0, -50000.0, -5000.0, 50000.0, 50000.0, 5000.0)
            .with_network_config(2000, true)
            .with_channel_frequencies([120.0, 60.0, 30.0, 10.0])
            .with_monitoring(true, true)
            .build()
    }

    /// Development configuration with extensive debugging
    pub fn development() -> GorcServerConfig {
        GorcConfigBuilder::new()
            .with_capacity(1000, 100)
            .with_virtualization(true, 0.5)
            .with_debug_logging(true)
            .with_monitoring(true, true)
            .build()
    }

    /// Low-resource configuration for smaller games
    pub fn low_resource() -> GorcServerConfig {
        GorcConfigBuilder::new()
            .with_capacity(5000, 500)
            .with_virtualization(true, 0.8)
            .with_network_config(500, true)
            .with_channel_frequencies([30.0, 15.0, 10.0, 2.0])
            .with_monitoring(true, false)
            .build()
    }

    /// Testing configuration with virtualization disabled
    pub fn testing() -> GorcServerConfig {
        GorcConfigBuilder::new()
            .with_capacity(100, 10)
            .with_virtualization(false, 1.0)
            .with_debug_logging(true)
            .with_monitoring(false, false)
            .build()
    }
}

/// Configuration validation
impl GorcServerConfig {
    /// Validates the configuration and returns any errors
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate general config
        if self.general.max_objects == 0 {
            return Err(ConfigValidationError::InvalidValue("max_objects must be > 0".to_string()));
        }

        if self.general.max_players == 0 {
            return Err(ConfigValidationError::InvalidValue("max_players must be > 0".to_string()));
        }

        if self.general.max_channels_per_object > 8 {
            return Err(ConfigValidationError::InvalidValue("max_channels_per_object cannot exceed 8".to_string()));
        }

        // Validate virtualization config
        if self.virtualization.enabled {
            if self.virtualization.density_threshold <= 0.0 {
                return Err(ConfigValidationError::InvalidValue("density_threshold must be > 0.0".to_string()));
            }

            if self.virtualization.overlap_threshold < 0.0 || self.virtualization.overlap_threshold > 1.0 {
                return Err(ConfigValidationError::InvalidValue("overlap_threshold must be between 0.0 and 1.0".to_string()));
            }

            if self.virtualization.max_virtual_zone_radius <= 0.0 {
                return Err(ConfigValidationError::InvalidValue("max_virtual_zone_radius must be > 0.0".to_string()));
            }
        }

        // Validate spatial config
        let (min_x, min_y, min_z, max_x, max_y, max_z) = self.spatial.world_bounds;
        if min_x >= max_x || min_y >= max_y || min_z >= max_z {
            return Err(ConfigValidationError::InvalidValue("world_bounds: min values must be < max values".to_string()));
        }

        if self.spatial.max_objects_per_leaf == 0 {
            return Err(ConfigValidationError::InvalidValue("max_objects_per_leaf must be greater than 0".to_string()));
        }

        if self.spatial.rebuild_threshold == 0 {
            return Err(ConfigValidationError::InvalidValue("rebuild_threshold must be greater than 0".to_string()));
        }

        // Validate network config
        if self.network.max_batch_size == 0 {
            return Err(ConfigValidationError::InvalidValue("max_batch_size must be > 0".to_string()));
        }

        for (i, &freq) in self.network.channel_frequencies.iter().enumerate() {
            if freq <= 0.0 || freq > 1000.0 {
                return Err(ConfigValidationError::InvalidValue(format!("channel_frequencies[{}] must be between 0.0 and 1000.0", i)));
            }
        }

        Ok(())
    }

    /// Optimizes the configuration based on system resources and expected load
    pub fn optimize_for_system(&mut self, cpu_cores: usize, memory_gb: usize, expected_players: usize) {
        // Adjust capacity based on system resources
        let capacity_multiplier = (cpu_cores * memory_gb).min(100);
        self.general.max_objects = (capacity_multiplier * 500).min(self.general.max_objects);
        self.general.max_players = (capacity_multiplier * 50).min(self.general.max_players);

        // Adjust virtualization settings based on expected load
        if expected_players > 1000 {
            self.virtualization.enabled = true;
            self.virtualization.density_threshold = 0.2; // More aggressive virtualization
            self.virtualization.max_objects_per_virtual_zone = 100;
        } else if expected_players > 100 {
            self.virtualization.enabled = true;
            self.virtualization.density_threshold = 0.5; // Standard virtualization
        } else {
            self.virtualization.enabled = false; // Disable for small games
        }

        // Adjust spatial index settings
        if memory_gb >= 16 {
            self.spatial.max_objects_per_leaf = 128;
            self.spatial.rebuild_threshold = 10_000;
            self.spatial.enable_caching = true;
        } else if memory_gb >= 8 {
            self.spatial.max_objects_per_leaf = 64;
            self.spatial.rebuild_threshold = 5_000;
            self.spatial.enable_caching = true;
        } else {
            self.spatial.max_objects_per_leaf = 32;
            self.spatial.rebuild_threshold = 2_000;
            self.spatial.enable_caching = false;
        }

        // Adjust network settings based on expected players
        if expected_players > 1000 {
            self.network.max_batch_size = 2000;
            self.network.enable_compression = true;
            self.network.compression_threshold = 512; // Compress smaller packets for high load
        }
    }
}

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
    #[error("Missing required configuration: {0}")]
    MissingRequired(String),
    #[error("Conflicting configuration: {0}")]
    Conflict(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_validation() {
        let config = GorcServerConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_builder() {
        let config = GorcConfigBuilder::new()
            .with_virtualization(true, 0.5)
            .with_capacity(1000, 100)
            .with_debug_logging(true)
            .build();

        assert!(config.virtualization.enabled);
        assert_eq!(config.virtualization.density_threshold, 0.5);
        assert_eq!(config.general.max_objects, 1000);
        assert_eq!(config.general.max_players, 100);
        assert!(config.general.debug_logging);
    }

    #[test]
    fn test_preset_configurations() {
        let high_perf = presets::high_performance();
        assert!(high_perf.validate().is_ok());
        assert!(high_perf.virtualization.enabled);
        assert_eq!(high_perf.general.max_objects, 50000);

        let dev = presets::development();
        assert!(dev.validate().is_ok());
        assert!(dev.general.debug_logging);

        let low_res = presets::low_resource();
        assert!(low_res.validate().is_ok());
        assert_eq!(low_res.network.max_batch_size, 500);

        let testing = presets::testing();
        assert!(testing.validate().is_ok());
        assert!(!testing.virtualization.enabled);
    }

    #[test]
    fn test_config_optimization() {
        let mut config = GorcServerConfig::default();
        config.optimize_for_system(8, 16, 2000);

        assert!(config.virtualization.enabled);
        assert_eq!(config.virtualization.density_threshold, 0.2);
        assert_eq!(config.spatial.max_objects_per_leaf, 128);
        assert_eq!(config.spatial.rebuild_threshold, 10_000);
        assert!(config.spatial.enable_caching);
    }

    #[test]
    fn test_invalid_config_validation() {
        let mut config = GorcServerConfig::default();

        // Test invalid density threshold
        config.virtualization.enabled = true;
        config.virtualization.density_threshold = -1.0;
        assert!(config.validate().is_err());

        // Test invalid overlap threshold
        config.virtualization.density_threshold = 0.5;
        config.virtualization.overlap_threshold = 1.5;
        assert!(config.validate().is_err());

        // Test invalid world bounds
        config.virtualization.overlap_threshold = 0.3;
        config.spatial.world_bounds = (10.0, 10.0, 10.0, 5.0, 5.0, 5.0);
        assert!(config.validate().is_err());
    }
}