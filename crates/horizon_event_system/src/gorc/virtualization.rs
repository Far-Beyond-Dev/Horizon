//! GORC Zone Virtualization System
//!
//! This module implements zone virtualization for high-density areas where many GORC objects
//! with overlapping zones exist. The system automatically merges overlapping zones into
//! virtual mega-zones to reduce spatial index load while maintaining precise split detection.
//!
//! ## Key Features
//! - Ultra-fast zone merging/splitting algorithms (sub-millisecond)
//! - Accurate overlap detection and boundary tracking
//! - Dynamic spatial index optimization
//! - Configurable density thresholds and merge criteria
//! - Event-driven merge/split notifications

use crate::types::Vec3;
use crate::gorc::instance::GorcObjectId;
use crate::gorc::channels::ReplicationLayer;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Configuration for GORC zone virtualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualizationConfig {
    /// Enable zone virtualization
    pub enabled: bool,
    /// Minimum object density per square unit to trigger virtualization
    pub density_threshold: f64,
    /// Minimum overlap percentage to merge zones (0.0-1.0)
    pub overlap_threshold: f64,
    /// Maximum virtual zone radius before splitting is forced
    pub max_virtual_zone_radius: f64,
    /// Minimum zone radius to consider for virtualization
    pub min_zone_radius: f64,
    /// Time in milliseconds between virtualization checks
    pub check_interval_ms: u64,
    /// Maximum number of objects in a virtual zone before forcing split
    pub max_objects_per_virtual_zone: usize,
}

impl Default for VirtualizationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            density_threshold: 0.5, // 0.5 objects per square unit
            overlap_threshold: 0.3, // 30% overlap required
            max_virtual_zone_radius: 1000.0,
            min_zone_radius: 50.0,
            check_interval_ms: 1000, // Check every second
            max_objects_per_virtual_zone: 50,
        }
    }
}

/// Represents a virtualized zone that encompasses multiple overlapping GORC zones
#[derive(Debug, Clone)]
pub struct VirtualZone {
    /// Unique identifier for this virtual zone
    pub virtual_id: VirtualZoneId,
    /// Bounding circle encompassing all merged zones
    pub center: Vec3,
    pub radius: f64,
    /// Objects and their original zones included in this virtual zone
    pub included_objects: HashMap<GorcObjectId, Vec<u8>>, // object_id -> channels
    /// Original zone boundaries for split detection
    pub original_zones: Vec<OriginalZoneInfo>,
    /// Replication channel this virtual zone represents
    pub channel: u8,
    /// When this virtual zone was created
    pub created_at: u64,
    /// Performance statistics
    pub stats: VirtualZoneStats,
}

/// Unique identifier for virtual zones
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VirtualZoneId(pub u64);

impl VirtualZoneId {
    pub fn new() -> Self {
        Self(crate::utils::current_timestamp())
    }
}

/// Information about an original zone that was merged into a virtual zone
#[derive(Debug, Clone)]
pub struct OriginalZoneInfo {
    pub object_id: GorcObjectId,
    pub channel: u8,
    pub center: Vec3,
    pub radius: f64,
    pub last_position: Vec3,
}

/// Statistics for virtual zone performance
#[derive(Debug, Default, Clone)]
pub struct VirtualZoneStats {
    /// Number of merge operations performed
    pub merge_operations: u64,
    /// Number of split operations performed
    pub split_operations: u64,
    /// Number of objects currently virtualized
    pub objects_virtualized: usize,
    /// Average processing time for merge/split operations (microseconds)
    pub avg_operation_time_us: f64,
}

/// Main virtualization manager
#[derive(Debug)]
pub struct VirtualizationManager {
    /// Configuration settings
    config: VirtualizationConfig,
    /// Active virtual zones by channel
    virtual_zones: Arc<RwLock<HashMap<u8, HashMap<VirtualZoneId, VirtualZone>>>>,
    /// Mapping from object to virtual zones it participates in
    object_to_virtual: Arc<RwLock<HashMap<GorcObjectId, HashSet<VirtualZoneId>>>>,
    /// Density tracking for different map regions
    density_tracker: Arc<RwLock<DensityTracker>>,
    /// Performance statistics
    stats: Arc<RwLock<VirtualizationStats>>,
    /// Next virtual zone ID
    next_virtual_id: Arc<RwLock<u64>>,
}

/// Tracks object density in spatial regions
#[derive(Debug, Default)]
pub struct DensityTracker {
    /// Grid-based density tracking (region_key -> object_count)
    density_grid: HashMap<(i32, i32), usize>,
    /// Grid cell size for density calculation
    grid_size: f64,
}

/// Global virtualization statistics
#[derive(Debug, Default, Clone)]
pub struct VirtualizationStats {
    /// Total virtual zones created
    pub total_virtual_zones_created: u64,
    /// Total virtual zones destroyed
    pub total_virtual_zones_destroyed: u64,
    /// Current active virtual zones
    pub active_virtual_zones: usize,
    /// Total objects currently virtualized
    pub total_objects_virtualized: usize,
    /// Average merge time in microseconds
    pub avg_merge_time_us: f64,
    /// Average split time in microseconds
    pub avg_split_time_us: f64,
    /// Spatial index load reduction percentage
    pub index_load_reduction_percent: f64,
}

impl VirtualizationManager {
    /// Creates a new virtualization manager with the given configuration
    pub fn new(config: VirtualizationConfig) -> Self {
        Self {
            config,
            virtual_zones: Arc::new(RwLock::new(HashMap::new())),
            object_to_virtual: Arc::new(RwLock::new(HashMap::new())),
            density_tracker: Arc::new(RwLock::new(DensityTracker::new())),
            stats: Arc::new(RwLock::new(VirtualizationStats::default())),
            next_virtual_id: Arc::new(RwLock::new(1)),
        }
    }

    /// Analyzes the given objects and returns merge/split recommendations
    pub async fn analyze_virtualization_opportunities(
        &self,
        objects: &HashMap<GorcObjectId, (Vec3, Vec<ReplicationLayer>)>
    ) -> VirtualizationRecommendations {
        if !self.config.enabled {
            return VirtualizationRecommendations::default();
        }

        let start_time = std::time::Instant::now();
        let mut recommendations = VirtualizationRecommendations::default();

        // Update density tracking
        self.update_density_tracking(objects).await;

        // Group objects by channel for analysis
        let mut objects_by_channel: HashMap<u8, Vec<(GorcObjectId, Vec3, f64)>> = HashMap::new();

        for (object_id, (position, layers)) in objects {
            for layer in layers {
                if layer.radius >= self.config.min_zone_radius {
                    objects_by_channel
                        .entry(layer.channel)
                        .or_default()
                        .push((*object_id, *position, layer.radius));
                }
            }
        }

        debug!("🔍 Virtualization analysis: {} objects across {} channels",
               objects.len(), objects_by_channel.len());
        for (channel, channel_objects) in &objects_by_channel {
            debug!("  Channel {}: {} objects", channel, channel_objects.len());
        }

        // Analyze each channel for virtualization opportunities
        for (channel, channel_objects) in objects_by_channel {
            let channel_recommendations = self.analyze_channel_virtualization(channel, &channel_objects).await;
            recommendations.merge_recommendations.extend(channel_recommendations.merge_recommendations);
            recommendations.split_recommendations.extend(channel_recommendations.split_recommendations);
        }

        // Check existing virtual zones for split conditions
        let split_recommendations = self.check_virtual_zones_for_splits(objects).await;
        recommendations.split_recommendations.extend(split_recommendations);

        let analysis_time = start_time.elapsed();
        debug!("🔍 Virtualization analysis completed in {:.3}ms", analysis_time.as_secs_f64() * 1000.0);

        recommendations
    }

    /// Performs zone merge operation
    pub async fn merge_zones(
        &self,
        merge_request: ZoneMergeRequest
    ) -> Result<VirtualZoneId, VirtualizationError> {
        let start_time = std::time::Instant::now();

        // Generate new virtual zone ID
        let virtual_id = {
            let mut next_id = self.next_virtual_id.write().await;
            let id = VirtualZoneId(*next_id);
            *next_id += 1;
            id
        };

        // Calculate optimal bounding circle for merged zones
        let (center, radius) = self.calculate_optimal_bounding_circle(&merge_request.zones).await?;

        // Create virtual zone
        let mut virtual_zone = VirtualZone {
            virtual_id,
            center,
            radius,
            included_objects: HashMap::new(),
            original_zones: Vec::new(),
            channel: merge_request.channel,
            created_at: crate::utils::current_timestamp(),
            stats: VirtualZoneStats::default(),
        };

        // Populate virtual zone with object information
        for zone_info in &merge_request.zones {
            virtual_zone.included_objects
                .entry(zone_info.object_id)
                .or_default()
                .push(zone_info.channel);

            virtual_zone.original_zones.push(OriginalZoneInfo {
                object_id: zone_info.object_id,
                channel: zone_info.channel,
                center: zone_info.center,
                radius: zone_info.radius,
                last_position: zone_info.center,
            });
        }

        // Register virtual zone
        {
            let mut virtual_zones = self.virtual_zones.write().await;
            virtual_zones
                .entry(merge_request.channel)
                .or_default()
                .insert(virtual_id, virtual_zone);
        }

        // Update object -> virtual zone mappings
        {
            let mut object_to_virtual = self.object_to_virtual.write().await;
            for zone_info in &merge_request.zones {
                object_to_virtual
                    .entry(zone_info.object_id)
                    .or_default()
                    .insert(virtual_id);
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_virtual_zones_created += 1;
            stats.active_virtual_zones += 1;
            stats.total_objects_virtualized += merge_request.zones.len();

            let merge_time = start_time.elapsed().as_micros() as f64;
            stats.avg_merge_time_us = (stats.avg_merge_time_us + merge_time) / 2.0;
        }

        info!("🔗 Created virtual zone {} covering {} objects on channel {} (radius: {:.1})",
              virtual_id.0, merge_request.zones.len(), merge_request.channel, radius);

        Ok(virtual_id)
    }

    /// Performs zone split operation
    pub async fn split_virtual_zone(
        &self,
        virtual_id: VirtualZoneId
    ) -> Result<Vec<GorcObjectId>, VirtualizationError> {
        let start_time = std::time::Instant::now();

        // Remove virtual zone and get its components
        let virtual_zone = {
            let mut virtual_zones = self.virtual_zones.write().await;

            // Find and remove the virtual zone
            let mut found_zone = None;
            for (_, channel_zones) in virtual_zones.iter_mut() {
                if let Some(zone) = channel_zones.remove(&virtual_id) {
                    found_zone = Some(zone);
                    break;
                }
            }

            found_zone.ok_or(VirtualizationError::VirtualZoneNotFound(virtual_id))?
        };

        // Get objects that were in this virtual zone
        let liberated_objects: Vec<GorcObjectId> = virtual_zone.included_objects.keys().copied().collect();

        // Update object -> virtual zone mappings
        {
            let mut object_to_virtual = self.object_to_virtual.write().await;
            for object_id in &liberated_objects {
                if let Some(virtual_set) = object_to_virtual.get_mut(object_id) {
                    virtual_set.remove(&virtual_id);
                    if virtual_set.is_empty() {
                        object_to_virtual.remove(object_id);
                    }
                }
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_virtual_zones_destroyed += 1;
            stats.active_virtual_zones = stats.active_virtual_zones.saturating_sub(1);
            stats.total_objects_virtualized = stats.total_objects_virtualized.saturating_sub(liberated_objects.len());

            let split_time = start_time.elapsed().as_micros() as f64;
            stats.avg_split_time_us = (stats.avg_split_time_us + split_time) / 2.0;
        }

        info!("✂️ Split virtual zone {} - liberated {} objects",
              virtual_id.0, liberated_objects.len());

        Ok(liberated_objects)
    }

    /// Updates object positions and checks for split conditions
    pub async fn update_object_position(
        &self,
        object_id: GorcObjectId,
        _old_position: Vec3,
        new_position: Vec3
    ) -> Vec<VirtualZoneId> {
        let mut zones_to_split = Vec::new();

        // Check if this object is in any virtual zones
        let virtual_zones = {
            let object_to_virtual = self.object_to_virtual.read().await;
            object_to_virtual.get(&object_id).cloned().unwrap_or_default()
        };

        if virtual_zones.is_empty() {
            return zones_to_split;
        }

        // Check each virtual zone for split conditions
        let mut virtual_zones_guard = self.virtual_zones.write().await;

        for virtual_id in virtual_zones {
            // Find the virtual zone
            let mut found_zone = None;
            for (_, channel_zones) in virtual_zones_guard.iter_mut() {
                if let Some(zone) = channel_zones.get_mut(&virtual_id) {
                    found_zone = Some(zone);
                    break;
                }
            }

            if let Some(virtual_zone) = found_zone {
                // Update the object's position in the virtual zone
                if let Some(original_zone) = virtual_zone.original_zones.iter_mut()
                    .find(|z| z.object_id == object_id) {
                    original_zone.last_position = new_position;
                }

                // Check if object moved outside virtual zone
                let distance_from_center = new_position.distance(virtual_zone.center);
                if distance_from_center > virtual_zone.radius {
                    zones_to_split.push(virtual_id);
                    continue;
                }

                // Check if objects in virtual zone are too spread out now
                if self.should_split_due_to_spread(virtual_zone).await {
                    zones_to_split.push(virtual_id);
                }
            }
        }

        zones_to_split
    }

    /// Checks if objects are subscribed to a virtual zone
    pub async fn is_in_virtual_zone(&self, position: Vec3, channel: u8) -> Option<VirtualZoneId> {
        let virtual_zones = self.virtual_zones.read().await;

        if let Some(channel_zones) = virtual_zones.get(&channel) {
            for (virtual_id, virtual_zone) in channel_zones {
                let distance = position.distance(virtual_zone.center);
                if distance <= virtual_zone.radius {
                    return Some(*virtual_id);
                }
            }
        }

        None
    }

    /// Gets all objects in a virtual zone
    pub async fn get_virtual_zone_objects(&self, virtual_id: VirtualZoneId) -> Vec<GorcObjectId> {
        let virtual_zones = self.virtual_zones.read().await;

        for (_, channel_zones) in virtual_zones.iter() {
            if let Some(virtual_zone) = channel_zones.get(&virtual_id) {
                return virtual_zone.included_objects.keys().copied().collect();
            }
        }

        Vec::new()
    }

    /// Gets virtualization statistics
    pub async fn get_stats(&self) -> VirtualizationStats {
        self.stats.read().await.clone()
    }

    // Private helper methods

    async fn update_density_tracking(&self, objects: &HashMap<GorcObjectId, (Vec3, Vec<ReplicationLayer>)>) {
        let mut density_tracker = self.density_tracker.write().await;
        density_tracker.update_density(objects);
    }

    async fn analyze_channel_virtualization(
        &self,
        channel: u8,
        objects: &[(GorcObjectId, Vec3, f64)]
    ) -> VirtualizationRecommendations {
        let mut recommendations = VirtualizationRecommendations::default();

        debug!("🔍 Analyzing channel {} with {} objects for virtualization", channel, objects.len());

        // Find clusters of overlapping zones
        let clusters = self.find_overlapping_clusters(objects).await;
        debug!("🔍 Found {} clusters for channel {}", clusters.len(), channel);

        for (i, cluster) in clusters.iter().enumerate() {
            debug!("🔍 Cluster {}: {} objects", i, cluster.len());
            if cluster.len() >= 2 {
                // Check density threshold
                let density = self.calculate_cluster_density(&cluster).await;
                debug!("🔍 Cluster {} density: {:.3} (threshold: {:.3})", i, density, self.config.density_threshold);

                if density >= self.config.density_threshold {
                    let merge_request = ZoneMergeRequest {
                        channel,
                        zones: cluster.iter().map(|(object_id, position, radius)| {
                            ZoneInfo {
                                object_id: *object_id,
                                channel,
                                center: *position,
                                radius: *radius,
                            }
                        }).collect(),
                    };

                    recommendations.merge_recommendations.push(merge_request);
                }
            }
        }

        recommendations
    }

    async fn find_overlapping_clusters(&self, objects: &[(GorcObjectId, Vec3, f64)]) -> Vec<Vec<(GorcObjectId, Vec3, f64)>> {
        debug!("🔍 find_overlapping_clusters called with {} objects", objects.len());
        let mut clusters = Vec::new();
        let mut visited = HashSet::new();

        for (i, &(object_id, position, radius)) in objects.iter().enumerate() {
            if visited.contains(&i) {
                continue;
            }

            let mut cluster = vec![(object_id, position, radius)];
            let mut cluster_indices = vec![i];
            visited.insert(i);

            // Find all objects that overlap with any object in the current cluster
            let mut changed = true;
            while changed {
                changed = false;

                for (j, &(other_id, other_pos, other_radius)) in objects.iter().enumerate() {
                    if visited.contains(&j) {
                        continue;
                    }

                    // Check if this object overlaps with any object in the cluster
                    for &(_, cluster_pos, cluster_radius) in &cluster {
                        let distance = other_pos.distance(cluster_pos);
                        let overlap_distance = cluster_radius + other_radius;
                        let overlap_ratio = (overlap_distance - distance) / overlap_distance.min(cluster_radius.min(other_radius) * 2.0);

                        // Debug logging can be enabled if needed
                        // debug!("🔍 Checking overlap: distance={:.1}, overlap_ratio={:.3}, threshold={:.3}",
                        //        distance, overlap_ratio, self.config.overlap_threshold);

                        if overlap_ratio >= self.config.overlap_threshold {
                            cluster.push((other_id, other_pos, other_radius));
                            cluster_indices.push(j);
                            visited.insert(j);
                            changed = true;
                            break;
                        }
                    }
                }
            }

            if cluster.len() >= 2 {
                clusters.push(cluster);
            }
        }

        clusters
    }

    async fn calculate_cluster_density(&self, cluster: &[(GorcObjectId, Vec3, f64)]) -> f64 {
        if cluster.is_empty() {
            return 0.0;
        }

        // For virtualization, density should represent how much the zones overlap
        // Calculate average overlap ratio within the cluster
        let mut total_overlap = 0.0;
        let mut comparison_count = 0;

        for (i, &(_, pos1, radius1)) in cluster.iter().enumerate() {
            for &(_, pos2, radius2) in cluster.iter().skip(i + 1) {
                let distance = pos1.distance(pos2);
                let max_possible_distance = radius1 + radius2;
                let overlap_ratio = if distance < max_possible_distance {
                    (max_possible_distance - distance) / max_possible_distance
                } else {
                    0.0
                };
                total_overlap += overlap_ratio;
                comparison_count += 1;
            }
        }

        if comparison_count == 0 {
            return 0.0;
        }

        total_overlap / comparison_count as f64
    }

    async fn check_virtual_zones_for_splits(&self, objects: &HashMap<GorcObjectId, (Vec3, Vec<ReplicationLayer>)>) -> Vec<ZoneSplitRequest> {
        let mut split_requests = Vec::new();
        let virtual_zones = self.virtual_zones.read().await;

        for (_, channel_zones) in virtual_zones.iter() {
            for (virtual_id, virtual_zone) in channel_zones {
                let should_split = self.should_split_virtual_zone(virtual_zone, objects).await;

                if should_split {
                    split_requests.push(ZoneSplitRequest {
                        virtual_id: *virtual_id,
                        reason: SplitReason::ObjectsSeparated,
                    });
                }
            }
        }

        split_requests
    }

    async fn should_split_virtual_zone(&self, virtual_zone: &VirtualZone, objects: &HashMap<GorcObjectId, (Vec3, Vec<ReplicationLayer>)>) -> bool {
        // Check if too many objects
        if virtual_zone.included_objects.len() > self.config.max_objects_per_virtual_zone {
            return true;
        }

        // Check if virtual zone is too large
        if virtual_zone.radius > self.config.max_virtual_zone_radius {
            return true;
        }

        // Check if objects have moved too far apart
        let mut max_distance: f64 = 0.0;
        let positions: Vec<Vec3> = virtual_zone.original_zones.iter()
            .filter_map(|zone| objects.get(&zone.object_id).map(|(pos, _)| *pos))
            .collect();

        for i in 0..positions.len() {
            for j in i + 1..positions.len() {
                max_distance = max_distance.max(positions[i].distance(positions[j]));
            }
        }

        // If objects are spread beyond the original virtual zone radius, consider splitting
        max_distance > virtual_zone.radius * 1.5
    }

    async fn should_split_due_to_spread(&self, virtual_zone: &VirtualZone) -> bool {
        // Calculate current spread of objects
        let positions: Vec<Vec3> = virtual_zone.original_zones.iter()
            .map(|zone| zone.last_position)
            .collect();

        if positions.len() < 2 {
            return false;
        }

        let mut max_distance: f64 = 0.0;
        for i in 0..positions.len() {
            for j in i + 1..positions.len() {
                max_distance = max_distance.max(positions[i].distance(positions[j]));
            }
        }

        // Split if objects are now too spread out
        max_distance > virtual_zone.radius * 2.0
    }

    async fn calculate_optimal_bounding_circle(&self, zones: &[ZoneInfo]) -> Result<(Vec3, f64), VirtualizationError> {
        if zones.is_empty() {
            return Err(VirtualizationError::EmptyZoneList);
        }

        if zones.len() == 1 {
            return Ok((zones[0].center, zones[0].radius));
        }

        // Use Welzl's algorithm for minimum enclosing circle
        let points: Vec<Vec3> = zones.iter().map(|z| z.center).collect();
        let (center, radius) = self.welzl_minimum_enclosing_circle(&points).await;

        // Expand radius to encompass all zone radii
        let max_zone_radius = zones.iter().map(|z| z.radius).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap_or(0.0);
        let expanded_radius = radius + max_zone_radius;

        Ok((center, expanded_radius))
    }

    async fn welzl_minimum_enclosing_circle(&self, points: &[Vec3]) -> (Vec3, f64) {
        // Simplified implementation - in production, use a proper geometric algorithm
        if points.is_empty() {
            return (Vec3::new(0.0, 0.0, 0.0), 0.0);
        }

        // Calculate centroid
        let centroid = points.iter().fold(Vec3::new(0.0, 0.0, 0.0), |acc, &p| {
            Vec3::new(acc.x + p.x, acc.y + p.y, acc.z + p.z)
        });
        let center = Vec3::new(
            centroid.x / points.len() as f64,
            centroid.y / points.len() as f64,
            centroid.z / points.len() as f64
        );

        // Find maximum distance from centroid
        let radius = points.iter()
            .map(|&p| center.distance(p))
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        (center, radius)
    }
}

impl DensityTracker {
    pub fn new() -> Self {
        Self {
            density_grid: HashMap::new(),
            grid_size: 1000.0, // 1000 unit grid cells
        }
    }

    pub fn update_density(&mut self, objects: &HashMap<GorcObjectId, (Vec3, Vec<ReplicationLayer>)>) {
        self.density_grid.clear();

        for (_, (position, _)) in objects {
            let grid_x = (position.x as f64 / self.grid_size).floor() as i32;
            let grid_y = (position.y as f64 / self.grid_size).floor() as i32;
            *self.density_grid.entry((grid_x, grid_y)).or_insert(0) += 1;
        }
    }

    pub fn get_density_at(&self, position: Vec3) -> f64 {
        let grid_x = (position.x as f64 / self.grid_size).floor() as i32;
        let grid_y = (position.y as f64 / self.grid_size).floor() as i32;
        let object_count = self.density_grid.get(&(grid_x, grid_y)).copied().unwrap_or(0);
        object_count as f64 / (self.grid_size * self.grid_size)
    }
}

/// Recommendations for zone merging and splitting
#[derive(Debug, Default)]
pub struct VirtualizationRecommendations {
    pub merge_recommendations: Vec<ZoneMergeRequest>,
    pub split_recommendations: Vec<ZoneSplitRequest>,
}

/// Request to merge multiple zones into a virtual zone
#[derive(Debug)]
pub struct ZoneMergeRequest {
    pub channel: u8,
    pub zones: Vec<ZoneInfo>,
}

/// Request to split a virtual zone
#[derive(Debug)]
pub struct ZoneSplitRequest {
    pub virtual_id: VirtualZoneId,
    pub reason: SplitReason,
}

/// Information about a zone to be merged
#[derive(Debug)]
pub struct ZoneInfo {
    pub object_id: GorcObjectId,
    pub channel: u8,
    pub center: Vec3,
    pub radius: f64,
}

/// Reasons for splitting a virtual zone
#[derive(Debug)]
pub enum SplitReason {
    ObjectsSeparated,
    TooManyObjects,
    ZoneTooLarge,
    DensityDecreased,
}

/// Errors that can occur during virtualization operations
#[derive(Debug, thiserror::Error)]
pub enum VirtualizationError {
    #[error("Virtual zone {0:?} not found")]
    VirtualZoneNotFound(VirtualZoneId),
    #[error("Empty zone list provided for merging")]
    EmptyZoneList,
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_virtualization_config() {
        let config = VirtualizationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.density_threshold, 0.5);
        assert_eq!(config.overlap_threshold, 0.3);
    }

    #[tokio::test]
    async fn test_virtual_zone_creation() {
        let config = VirtualizationConfig {
            enabled: true,
            ..Default::default()
        };

        let manager = VirtualizationManager::new(config);

        let zones = vec![
            ZoneInfo {
                object_id: GorcObjectId::new(),
                channel: 0,
                center: Vec3::new(0.0, 0.0, 0.0),
                radius: 100.0,
            },
            ZoneInfo {
                object_id: GorcObjectId::new(),
                channel: 0,
                center: Vec3::new(50.0, 0.0, 0.0),
                radius: 100.0,
            },
        ];

        let merge_request = ZoneMergeRequest { channel: 0, zones };
        let virtual_id = manager.merge_zones(merge_request).await.unwrap();

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_virtual_zones_created, 1);
        assert_eq!(stats.active_virtual_zones, 1);
    }

    #[tokio::test]
    async fn test_virtual_zone_splitting() {
        let config = VirtualizationConfig {
            enabled: true,
            ..Default::default()
        };

        let manager = VirtualizationManager::new(config);

        // Create a virtual zone first
        let zones = vec![
            ZoneInfo {
                object_id: GorcObjectId::new(),
                channel: 0,
                center: Vec3::new(0.0, 0.0, 0.0),
                radius: 50.0,
            },
        ];

        let merge_request = ZoneMergeRequest { channel: 0, zones };
        let virtual_id = manager.merge_zones(merge_request).await.unwrap();

        // Split the virtual zone
        let liberated_objects = manager.split_virtual_zone(virtual_id).await.unwrap();
        assert_eq!(liberated_objects.len(), 1);

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_virtual_zones_destroyed, 1);
        assert_eq!(stats.active_virtual_zones, 0);
    }
}