//! # GORC Zone Management System
//!
//! This module manages the zones around each object instance for efficient
//! proximity-based replication. Each object has multiple concentric zones
//! corresponding to different replication channels.

use crate::types::Vec3;
use crate::gorc::channels::ReplicationLayer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single zone around an object for a specific replication channel
#[derive(Debug, Clone)]
pub struct ObjectZone {
    /// The replication channel this zone corresponds to
    pub channel: u8,
    /// Center position of the zone (object position)
    pub center: Vec3,
    /// Radius of the zone
    pub radius: f64,
    /// Properties replicated in this zone
    pub properties: Vec<String>,
    /// Update frequency for this zone
    pub frequency: f64,
    /// Whether the zone is active
    pub active: bool,
}

impl ObjectZone {
    /// Creates a new object zone
    pub fn new(channel: u8, center: Vec3, layer: &ReplicationLayer) -> Self {
        Self {
            channel,
            center,
            radius: layer.radius,
            properties: layer.properties.clone(),
            frequency: layer.frequency,
            active: true,
        }
    }

    /// Updates the center position of the zone
    pub fn update_center(&mut self, new_center: Vec3) {
        self.center = new_center;
    }

    /// Checks if a position is within this zone
    pub fn contains(&self, position: Vec3) -> bool {
        if !self.active {
            return false;
        }
        
        self.center.distance(position) <= self.radius
    }

    /// Checks if a position is within this zone with hysteresis
    /// This prevents rapid subscribe/unsubscribe cycles at zone boundaries
    pub fn contains_with_hysteresis(&self, position: Vec3, is_currently_inside: bool) -> bool {
        if !self.active {
            return false;
        }

        let distance = self.center.distance(position);
        let hysteresis_factor = 0.05; // 5% hysteresis
        let hysteresis_distance = self.radius * hysteresis_factor;

        if is_currently_inside {
            // If already inside, use larger radius (harder to exit)
            distance <= (self.radius + hysteresis_distance)
        } else {
            // If outside, use smaller radius (easier to enter)
            distance <= (self.radius - hysteresis_distance)
        }
    }

    /// Gets the distance from the zone center to a position
    pub fn distance_to(&self, position: Vec3) -> f64 {
        self.center.distance(position)
    }

    /// Gets how much of the zone a position penetrates (0.0 = edge, 1.0 = center)
    pub fn penetration_factor(&self, position: Vec3) -> f64 {
        if !self.contains(position) {
            return 0.0;
        }

        let distance = self.center.distance(position);
        if distance >= self.radius {
            0.0
        } else {
            1.0 - (distance / self.radius)
        }
    }

    /// Activates or deactivates the zone
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
}

/// Manages all zones for a single object instance
#[derive(Debug, Clone)]
pub struct ZoneManager {
    /// All zones for this object, indexed by channel
    zones: HashMap<u8, ObjectZone>,
    /// Current center position of all zones
    center: Vec3,
    /// Zone statistics
    stats: ZoneStats,
}

impl ZoneManager {
    /// Creates a new zone manager for an object
    pub fn new(center: Vec3, layers: Vec<ReplicationLayer>) -> Self {
        let mut zones = HashMap::new();
        
        for layer in layers {
            let zone = ObjectZone::new(layer.channel, center, &layer);
            zones.insert(layer.channel, zone);
        }

        Self {
            zones,
            center,
            stats: ZoneStats::default(),
        }
    }

    /// Updates the center position of all zones
    pub fn update_position(&mut self, new_center: Vec3) {
        let movement_distance = self.center.distance(new_center);
        self.center = new_center;
        
        for zone in self.zones.values_mut() {
            zone.update_center(new_center);
        }
        
        self.stats.position_updates += 1;
        self.stats.total_movement_distance += movement_distance;
    }

    /// Checks if a position is within a specific zone
    pub fn is_in_zone(&self, position: Vec3, channel: u8) -> bool {
        self.zones
            .get(&channel)
            .map(|zone| zone.contains(position))
            .unwrap_or(false)
    }

    /// Checks if a position is within a zone with hysteresis
    pub fn is_in_zone_with_hysteresis(
        &self, 
        position: Vec3, 
        channel: u8, 
        currently_inside: bool
    ) -> bool {
        self.zones
            .get(&channel)
            .map(|zone| zone.contains_with_hysteresis(position, currently_inside))
            .unwrap_or(false)
    }

    /// Gets all channels that contain a given position
    pub fn get_containing_channels(&self, position: Vec3) -> Vec<u8> {
        self.zones
            .iter()
            .filter(|(_, zone)| zone.contains(position))
            .map(|(&channel, _)| channel)
            .collect()
    }

    /// Gets the distance to the nearest zone edge for a position
    pub fn distance_to_nearest_zone(&self, position: Vec3) -> f64 {
        self.zones
            .values()
            .map(|zone| {
                let distance_to_center = zone.distance_to(position);
                (distance_to_center - zone.radius).abs()
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(f64::INFINITY)
    }

    /// Gets zone information for a specific channel
    pub fn get_zone(&self, channel: u8) -> Option<&ObjectZone> {
        self.zones.get(&channel)
    }

    /// Gets mutable zone information for a specific channel
    pub fn get_zone_mut(&mut self, channel: u8) -> Option<&mut ObjectZone> {
        self.zones.get_mut(&channel)
    }

    /// Gets all zones
    pub fn get_zones(&self) -> &HashMap<u8, ObjectZone> {
        &self.zones
    }

    /// Activates or deactivates a specific zone
    pub fn set_zone_active(&mut self, channel: u8, active: bool) -> bool {
        if let Some(zone) = self.zones.get_mut(&channel) {
            zone.set_active(active);
            true
        } else {
            false
        }
    }

    /// Activates or deactivates all zones
    pub fn set_all_zones_active(&mut self, active: bool) {
        for zone in self.zones.values_mut() {
            zone.set_active(active);
        }
    }

    /// Gets the maximum zone radius
    pub fn max_radius(&self) -> f64 {
        self.zones
            .values()
            .map(|zone| zone.radius)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Gets the minimum zone radius  
    pub fn min_radius(&self) -> f64 {
        self.zones
            .values()
            .map(|zone| zone.radius)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Checks if any zone contains the position
    pub fn contains_position(&self, position: Vec3) -> bool {
        self.zones.values().any(|zone| zone.contains(position))
    }

    /// Gets detailed zone analysis for a position
    pub fn analyze_position(&self, position: Vec3) -> ZoneAnalysis {
        let mut analysis = ZoneAnalysis {
            position,
            object_center: self.center,
            distance_to_object: self.center.distance(position),
            containing_channels: Vec::new(),
            zone_penetrations: HashMap::new(),
            nearest_zone_distance: f64::INFINITY,
        };

        for (&channel, zone) in &self.zones {
            if zone.contains(position) {
                analysis.containing_channels.push(channel);
                analysis.zone_penetrations.insert(channel, zone.penetration_factor(position));
            }

            let distance_to_edge = (zone.distance_to(position) - zone.radius).abs();
            if distance_to_edge < analysis.nearest_zone_distance {
                analysis.nearest_zone_distance = distance_to_edge;
            }
        }

        analysis.containing_channels.sort();
        analysis
    }

    /// Gets statistics for this zone manager
    pub fn get_stats(&self) -> &ZoneStats {
        &self.stats
    }

    /// Resets statistics
    pub fn reset_stats(&mut self) {
        self.stats = ZoneStats::default();
    }
}

/// Detailed analysis of a position relative to object zones
#[derive(Debug, Clone)]
pub struct ZoneAnalysis {
    /// The analyzed position
    pub position: Vec3,
    /// Center of the object
    pub object_center: Vec3,
    /// Distance from position to object center
    pub distance_to_object: f64,
    /// Channels whose zones contain this position
    pub containing_channels: Vec<u8>,
    /// How deeply the position penetrates each zone (0.0 = edge, 1.0 = center)
    pub zone_penetrations: HashMap<u8, f64>,
    /// Distance to the nearest zone edge
    pub nearest_zone_distance: f64,
}

impl ZoneAnalysis {
    /// Checks if the position is in any zone
    pub fn is_in_any_zone(&self) -> bool {
        !self.containing_channels.is_empty()
    }

    /// Gets the highest priority channel containing this position
    pub fn highest_priority_channel(&self) -> Option<u8> {
        self.containing_channels.first().copied()
    }

    /// Gets the maximum penetration factor across all zones
    pub fn max_penetration(&self) -> f64 {
        self.zone_penetrations
            .values()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .copied()
            .unwrap_or(0.0)
    }

    /// Gets the penetration factor for a specific channel
    pub fn penetration_for_channel(&self, channel: u8) -> f64 {
        self.zone_penetrations.get(&channel).copied().unwrap_or(0.0)
    }
}

/// Statistics for zone management
#[derive(Debug, Default, Clone)]
pub struct ZoneStats {
    /// Number of position updates
    pub position_updates: u64,
    /// Total distance moved
    pub total_movement_distance: f64,
    /// Average movement per update
    pub avg_movement_per_update: f64,
    /// Number of zone boundary crossings
    pub boundary_crossings: u64,
}

impl ZoneStats {
    /// Updates the average movement calculation
    pub fn update_averages(&mut self) {
        if self.position_updates > 0 {
            self.avg_movement_per_update = self.total_movement_distance / self.position_updates as f64;
        }
    }
}

/// Configuration for zone behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneConfig {
    /// Global hysteresis factor (0.0 to 1.0)
    pub hysteresis_factor: f64,
    /// Minimum update interval per zone in milliseconds
    pub min_update_interval_ms: u64,
    /// Maximum subscribers per zone before degrading performance
    pub max_subscribers_per_zone: usize,
    /// Whether to use adaptive zone sizing
    pub adaptive_sizing: bool,
    /// Factor to scale zones based on subscriber count
    pub adaptive_scale_factor: f64,
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self {
            hysteresis_factor: 0.05,
            min_update_interval_ms: 16, // ~60 FPS
            max_subscribers_per_zone: 100,
            adaptive_sizing: false,
            adaptive_scale_factor: 1.0,
        }
    }
}

/// Advanced zone manager with configuration and optimization features
#[derive(Debug)]
pub struct AdvancedZoneManager {
    /// Base zone manager
    base: ZoneManager,
    /// Configuration for zone behavior
    config: ZoneConfig,
    /// Subscriber counts per channel for adaptive sizing
    subscriber_counts: HashMap<u8, usize>,
    /// Performance metrics
    performance_metrics: ZonePerformanceMetrics,
}

impl AdvancedZoneManager {
    /// Creates a new advanced zone manager
    pub fn new(center: Vec3, layers: Vec<ReplicationLayer>, config: ZoneConfig) -> Self {
        Self {
            base: ZoneManager::new(center, layers),
            config,
            subscriber_counts: HashMap::new(),
            performance_metrics: ZonePerformanceMetrics::default(),
        }
    }

    /// Updates subscriber count for adaptive zone sizing
    pub fn update_subscriber_count(&mut self, channel: u8, count: usize) {
        self.subscriber_counts.insert(channel, count);
        
        if self.config.adaptive_sizing {
            self.adjust_zone_size(channel, count);
        }
    }

    /// Adjusts zone size based on subscriber count
    fn adjust_zone_size(&mut self, channel: u8, subscriber_count: usize) {
        if let Some(zone) = self.base.get_zone_mut(channel) {
            let original_radius = zone.radius;
            
            // Scale zone based on subscriber density
            let scale_factor = if subscriber_count > self.config.max_subscribers_per_zone {
                // Shrink zone if too many subscribers
                1.0 - (subscriber_count as f64 / self.config.max_subscribers_per_zone as f64 - 1.0) * self.config.adaptive_scale_factor
            } else {
                // Expand zone if few subscribers
                1.0 + (1.0 - subscriber_count as f64 / self.config.max_subscribers_per_zone as f64) * self.config.adaptive_scale_factor * 0.5
            };
            
            zone.radius = original_radius * scale_factor.clamp(0.5, 2.0);
            
            self.performance_metrics.zone_adjustments += 1;
        }
    }

    /// Checks if position is in zone with advanced hysteresis
    pub fn is_in_zone_advanced(&self, position: Vec3, channel: u8, currently_inside: bool) -> bool {
        if let Some(zone) = self.base.get_zone(channel) {
            let distance = zone.distance_to(position);
            let hysteresis_distance = zone.radius * self.config.hysteresis_factor;
            
            if currently_inside {
                distance <= (zone.radius + hysteresis_distance)
            } else {
                distance <= (zone.radius - hysteresis_distance)
            }
        } else {
            false
        }
    }

    /// Gets performance metrics
    pub fn get_performance_metrics(&self) -> &ZonePerformanceMetrics {
        &self.performance_metrics
    }

    /// Delegates to base zone manager
    pub fn update_position(&mut self, new_center: Vec3) {
        self.base.update_position(new_center);
        self.performance_metrics.position_updates += 1;
    }

    /// Delegates to base zone manager
    pub fn is_in_zone(&self, position: Vec3, channel: u8) -> bool {
        self.base.is_in_zone(position, channel)
    }

    /// Delegates to base zone manager
    pub fn get_containing_channels(&self, position: Vec3) -> Vec<u8> {
        self.base.get_containing_channels(position)
    }

    /// Delegates to base zone manager
    pub fn analyze_position(&self, position: Vec3) -> ZoneAnalysis {
        self.base.analyze_position(position)
    }

    /// Gets the base zone manager
    pub fn base(&self) -> &ZoneManager {
        &self.base
    }

    /// Gets the base zone manager mutably
    pub fn base_mut(&mut self) -> &mut ZoneManager {
        &mut self.base
    }
}

/// Performance metrics for zone management
#[derive(Debug, Default, Clone)]
pub struct ZonePerformanceMetrics {
    /// Number of position updates processed
    pub position_updates: u64,
    /// Number of zone size adjustments made
    pub zone_adjustments: u64,
    /// Number of boundary crossing events
    pub boundary_crossings: u64,
    /// Average time spent in zone calculations (microseconds)
    pub avg_calculation_time_us: f64,
    /// Peak subscriber count across all zones
    pub peak_subscriber_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gorc::channels::{ReplicationLayer, CompressionType};

    #[test]
    fn test_object_zone_creation() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let layer = ReplicationLayer::new(
            0,
            100.0,
            30.0,
            vec!["position".to_string()],
            CompressionType::Delta,
        );
        
        let zone = ObjectZone::new(0, center, &layer);
        
        assert_eq!(zone.channel, 0);
        assert_eq!(zone.center, center);
        assert_eq!(zone.radius, 100.0);
        assert!(zone.active);
    }

    #[test]
    fn test_zone_contains() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let layer = ReplicationLayer::new(
            0,
            100.0,
            30.0,
            vec!["position".to_string()],
            CompressionType::Delta,
        );
        
        let zone = ObjectZone::new(0, center, &layer);
        
        assert!(zone.contains(Vec3::new(50.0, 0.0, 0.0)));
        assert!(zone.contains(Vec3::new(0.0, 50.0, 0.0)));
        assert!(!zone.contains(Vec3::new(150.0, 0.0, 0.0)));
    }

    #[test]
    fn test_zone_hysteresis() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let layer = ReplicationLayer::new(
            0,
            100.0,
            30.0,
            vec!["position".to_string()],
            CompressionType::Delta,
        );
        
        let zone = ObjectZone::new(0, center, &layer);
        let edge_position = Vec3::new(98.0, 0.0, 0.0);
        
        // When inside, hysteresis allows staying in
        assert!(zone.contains_with_hysteresis(edge_position, true));
        
        // When outside, hysteresis prevents entering
        assert!(!zone.contains_with_hysteresis(edge_position, false));
    }

    #[test]
    fn test_zone_manager() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let layers = vec![
            ReplicationLayer::new(0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta),
            ReplicationLayer::new(1, 150.0, 30.0, vec!["animation".to_string()], CompressionType::Lz4),
        ];
        
        let manager = ZoneManager::new(center, layers);
        
        let close_pos = Vec3::new(25.0, 0.0, 0.0);
        let far_pos = Vec3::new(100.0, 0.0, 0.0);
        
        assert!(manager.is_in_zone(close_pos, 0));
        assert!(manager.is_in_zone(close_pos, 1));
        assert!(!manager.is_in_zone(far_pos, 0));
        assert!(manager.is_in_zone(far_pos, 1));
    }

    #[test]
    fn test_zone_analysis() {
        let center = Vec3::new(0.0, 0.0, 0.0);
        let layers = vec![
            ReplicationLayer::new(0, 50.0, 60.0, vec!["position".to_string()], CompressionType::Delta),
            ReplicationLayer::new(1, 100.0, 30.0, vec!["animation".to_string()], CompressionType::Lz4),
        ];
        
        let manager = ZoneManager::new(center, layers);
        let analysis = manager.analyze_position(Vec3::new(25.0, 0.0, 0.0));
        
        assert!(analysis.is_in_any_zone());
        assert_eq!(analysis.containing_channels, vec![0, 1]);
        assert!(analysis.max_penetration() > 0.0);
        assert_eq!(analysis.highest_priority_channel(), Some(0));
    }
}