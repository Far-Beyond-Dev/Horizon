/// GORC channel manager and coordination
use super::channel::{ReplicationChannel, ChannelStats};
use super::layer::ReplicationLayer;
use super::types::{ReplicationPriority, GorcError};
use crate::types::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Main GORC manager that coordinates all replication channels
pub struct GorcManager {
    /// All configured replication channels
    channels: Arc<RwLock<HashMap<u8, ReplicationChannel>>>,
    /// Cached replication layers for quick access
    layers: Arc<RwLock<HashMap<String, ReplicationLayer>>>,
    /// Global GORC statistics
    stats: Arc<RwLock<GorcStats>>,
    /// System configuration
    #[allow(dead_code)]
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

    /// Creates a GORC manager with custom configuration
    pub fn with_config(config: GorcConfig) -> Self {
        let mut manager = Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            layers: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(GorcStats::default())),
            config,
        };

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
        pos1.distance(pos2)
    }

    /// Updates channel statistics
    pub async fn update_channel_stats(&self, channel_id: u8, bytes_sent: u64, frequency: f32) {
        if let Some(mut channel) = self.get_channel(channel_id).await {
            channel.stats.add_bytes_transmitted(bytes_sent);
            channel.stats.update_frequency(frequency);
            
            // Update the channel in storage
            let mut channels = self.channels.write().await;
            channels.insert(channel_id, channel);
        }
    }

    /// Gets comprehensive GORC statistics
    pub async fn get_stats(&self) -> GorcStats {
        let mut stats = self.stats.read().await.clone();
        
        // Update with current channel stats
        let channels = self.channels.read().await;
        stats.channel_stats.clear();
        
        for (id, channel) in channels.iter() {
            stats.channel_stats.insert(*id, channel.stats.clone());
        }
        
        stats
    }

    /// Gets performance report
    pub async fn get_performance_report(&self) -> PerformanceReport {
        let stats = self.get_stats().await;
        let channels = self.channels.read().await;
        
        let mut channel_reports = HashMap::new();
        for (id, channel) in channels.iter() {
            let report = ChannelPerformanceReport {
                channel_id: *id,
                efficiency: channel.stats.efficiency_ratio(channel.frequency_range.1),
                avg_latency: channel.stats.avg_latency_ms,
                throughput: channel.stats.avg_frequency,
                subscriber_count: channel.stats.subscriber_count,
            };
            channel_reports.insert(*id, report);
        }
        
        PerformanceReport {
            overall_efficiency: self.calculate_overall_efficiency(&channel_reports).await,
            network_utilization: stats.network_utilization,
            channel_reports,
            timestamp: crate::utils::current_timestamp(),
        }
    }

    /// Calculates overall system efficiency
    async fn calculate_overall_efficiency(&self, channel_reports: &HashMap<u8, ChannelPerformanceReport>) -> f32 {
        if channel_reports.is_empty() {
            return 1.0;
        }
        
        let sum: f32 = channel_reports.values().map(|r| r.efficiency).sum();
        sum / channel_reports.len() as f32
    }

    /// Optimizes all channels
    pub async fn optimize_channels(&mut self) -> Result<(), GorcError> {
        let mut channels = self.channels.write().await;
        
        for channel in channels.values_mut() {
            channel.optimize();
        }
        
        Ok(())
    }

    /// Sets channel active/inactive
    pub async fn set_channel_active(&self, channel_id: u8, active: bool) -> Result<(), GorcError> {
        let mut channels = self.channels.write().await;
        
        if let Some(channel) = channels.get_mut(&channel_id) {
            channel.set_active(active);
            Ok(())
        } else {
            Err(GorcError::Channel(format!("Channel {} not found", channel_id)))
        }
    }
}

impl Default for GorcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for the GORC system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GorcConfig {
    /// Maximum number of concurrent objects
    pub max_objects: usize,
    /// Default compression type
    pub default_compression: super::types::CompressionType,
    /// Enable adaptive frequency scaling
    pub adaptive_frequency: bool,
    /// Network optimization level
    pub optimization_level: u8,
}

impl Default for GorcConfig {
    fn default() -> Self {
        Self {
            max_objects: 10000,
            default_compression: super::types::CompressionType::Lz4,
            adaptive_frequency: true,
            optimization_level: 2,
        }
    }
}

/// Global GORC statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GorcStats {
    /// Statistics per channel
    pub channel_stats: HashMap<u8, ChannelStats>,
    /// Total objects tracked
    pub total_objects: usize,
    /// Overall network utilization
    pub network_utilization: f32,
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// Total replication events processed
    pub total_events_processed: u64,
}

/// Performance report for a single channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPerformanceReport {
    pub channel_id: u8,
    pub efficiency: f32,
    pub avg_latency: f32,
    pub throughput: f32,
    pub subscriber_count: usize,
}

/// Overall system performance report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub overall_efficiency: f32,
    pub network_utilization: f32,
    pub channel_reports: HashMap<u8, ChannelPerformanceReport>,
    pub timestamp: u64,
}

impl PerformanceReport {
    /// Returns true if the system is performing well
    pub fn is_healthy(&self) -> bool {
        self.overall_efficiency > 0.7 && self.network_utilization < 0.9
    }
    
    /// Gets performance recommendations
    pub fn get_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        
        if self.overall_efficiency < 0.6 {
            recommendations.push("System efficiency is low - consider optimizing channels".to_string());
        }
        
        if self.network_utilization > 0.8 {
            recommendations.push("High network utilization - consider reducing update frequencies".to_string());
        }
        
        for (id, report) in &self.channel_reports {
            if report.efficiency < 0.5 {
                recommendations.push(format!("Channel {} has poor efficiency", id));
            }
            if report.avg_latency > 100.0 {
                recommendations.push(format!("Channel {} has high latency", id));
            }
        }
        
        recommendations
    }
    
    /// Gets health score (0.0 to 1.0)
    pub fn health_score(&self) -> f32 {
        let efficiency_score = self.overall_efficiency;
        let network_score = 1.0 - self.network_utilization;
        
        (efficiency_score + network_score) / 2.0
    }
}