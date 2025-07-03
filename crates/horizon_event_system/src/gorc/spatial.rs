//! # Spatial Partitioning System
//!
//! This module implements efficient spatial partitioning for GORC subscription management,
//! including quadtree-based spatial indexing and efficient neighbor queries.

use crate::types::{PlayerId, Position, RegionBounds};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum depth for quadtree subdivision
const MAX_QUADTREE_DEPTH: u8 = 10;
/// Maximum objects per quadtree node before subdivision
const MAX_OBJECTS_PER_NODE: usize = 16;

/// Spatial query parameters
#[derive(Debug, Clone)]
pub struct SpatialQuery {
    /// Center position of the query
    pub center: Position,
    /// Query radius
    pub radius: f32,
    /// Optional filters for the query
    pub filters: QueryFilters,
}

/// Filters that can be applied to spatial queries
#[derive(Debug, Clone, Default)]
pub struct QueryFilters {
    /// Include only specific players
    pub include_players: Option<HashSet<PlayerId>>,
    /// Exclude specific players
    pub exclude_players: Option<HashSet<PlayerId>>,
    /// Maximum number of results to return
    pub max_results: Option<usize>,
    /// Minimum distance from query center
    pub min_distance: Option<f32>,
}

/// Result of a spatial query
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Player ID
    pub player_id: PlayerId,
    /// Player position
    pub position: Position,
    /// Distance from query center
    pub distance: f32,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Node in the quadtree spatial index
#[derive(Debug, Clone)]
pub struct QuadTreeNode {
    /// Bounding box for this node
    pub bounds: RegionBounds,
    /// Objects in this node (if leaf)
    pub objects: Vec<SpatialObject>,
    /// Child nodes (NW, NE, SW, SE)
    pub children: Option<Box<[QuadTreeNode; 4]>>,
    /// Depth of this node
    pub depth: u8,
    /// Whether this is a leaf node
    pub is_leaf: bool,
}

impl QuadTreeNode {
    /// Creates a new quadtree node
    pub fn new(bounds: RegionBounds, depth: u8) -> Self {
        Self {
            bounds,
            objects: Vec::new(),
            children: None,
            depth,
            is_leaf: true,
        }
    }

    /// Inserts an object into the quadtree
    pub fn insert(&mut self, object: SpatialObject) {
        if !self.contains_point(object.position) {
            return; // Object is outside this node's bounds
        }

        if self.is_leaf {
            self.objects.push(object);

            // Check if we need to subdivide
            if self.objects.len() > MAX_OBJECTS_PER_NODE && self.depth < MAX_QUADTREE_DEPTH {
                self.subdivide();
            }
        } else {
            // Insert into appropriate child
            if let Some(ref mut children) = self.children {
                for child in children.iter_mut() {
                    if child.contains_point(object.position) {
                        child.insert(object);
                        break;
                    }
                }
            }
        }
    }

    /// Removes an object from the quadtree
    pub fn remove(&mut self, player_id: PlayerId) -> bool {
        if self.is_leaf {
            let initial_len = self.objects.len();
            self.objects.retain(|obj| obj.player_id != player_id);
            self.objects.len() != initial_len
        } else if let Some(ref mut children) = self.children {
            let mut removed = false;
            for child in children.iter_mut() {
                if child.remove(player_id) {
                    removed = true;
                }
            }
            
            // Check if we can merge children back into this node
            if removed {
                self.try_merge();
            }
            
            removed
        } else {
            false
        }
    }

    /// Queries objects within a radius of a point
    pub fn query_radius(&self, center: Position, radius: f32, results: &mut Vec<SpatialObject>) {
        // Check if this node's bounds intersect with the query circle
        if !self.bounds_intersect_circle(center, radius) {
            return;
        }

        if self.is_leaf {
            // Check all objects in this leaf
            for object in &self.objects {
                let distance = Self::distance(center, object.position);
                if distance <= radius {
                    results.push(object.clone());
                }
            }
        } else if let Some(ref children) = self.children {
            // Recursively query children
            for child in children.iter() {
                child.query_radius(center, radius, results);
            }
        }
    }

    /// Subdivides this node into four children
    fn subdivide(&mut self) {
        if !self.is_leaf || self.depth >= MAX_QUADTREE_DEPTH {
            return;
        }

        let mid_x = (self.bounds.min_x + self.bounds.max_x) / 2.0;
        let mid_z = (self.bounds.min_z + self.bounds.max_z) / 2.0;

        // Create four children: NW, NE, SW, SE
        let children = Box::new([
            // Northwest
            QuadTreeNode::new(
                RegionBounds {
                    min_x: self.bounds.min_x,
                    max_x: mid_x,
                    min_y: self.bounds.min_y,
                    max_y: self.bounds.max_y,
                    min_z: mid_z,
                    max_z: self.bounds.max_z,
                },
                self.depth + 1,
            ),
            // Northeast
            QuadTreeNode::new(
                RegionBounds {
                    min_x: mid_x,
                    max_x: self.bounds.max_x,
                    min_y: self.bounds.min_y,
                    max_y: self.bounds.max_y,
                    min_z: mid_z,
                    max_z: self.bounds.max_z,
                },
                self.depth + 1,
            ),
            // Southwest
            QuadTreeNode::new(
                RegionBounds {
                    min_x: self.bounds.min_x,
                    max_x: mid_x,
                    min_y: self.bounds.min_y,
                    max_y: self.bounds.max_y,
                    min_z: self.bounds.min_z,
                    max_z: mid_z,
                },
                self.depth + 1,
            ),
            // Southeast
            QuadTreeNode::new(
                RegionBounds {
                    min_x: mid_x,
                    max_x: self.bounds.max_x,
                    min_y: self.bounds.min_y,
                    max_y: self.bounds.max_y,
                    min_z: self.bounds.min_z,
                    max_z: mid_z,
                },
                self.depth + 1,
            ),
        ]);

        // Move objects to children
        let objects = std::mem::take(&mut self.objects);
        self.children = Some(children);
        self.is_leaf = false;

        // Reinsert objects into appropriate children
        for object in objects {
            self.insert(object);
        }
    }

    /// Attempts to merge children back into this node if they're sparse enough
    fn try_merge(&mut self) {
        if self.is_leaf || self.children.is_none() {
            return;
        }

        let total_objects: usize = if let Some(ref children) = self.children {
            children.iter()
                .map(|child| if child.is_leaf { child.objects.len() } else { usize::MAX })
                .sum()
        } else {
            return;
        };

        // Only merge if all children are leaves and total objects is small
        if total_objects <= MAX_OBJECTS_PER_NODE / 2 {
            if let Some(children) = self.children.take() {
                self.objects.clear();
                for child in children.iter() {
                    if child.is_leaf {
                        self.objects.extend(child.objects.iter().cloned());
                    }
                }
                self.is_leaf = true;
            }
        }
    }

    /// Checks if a point is within this node's bounds
    fn contains_point(&self, position: Position) -> bool {
        position.x >= self.bounds.min_x && position.x <= self.bounds.max_x &&
        position.y >= self.bounds.min_y && position.y <= self.bounds.max_y &&
        position.z >= self.bounds.min_z && position.z <= self.bounds.max_z
    }

    /// Checks if this node's bounds intersect with a circle
    fn bounds_intersect_circle(&self, center: Position, radius: f32) -> bool {
        // Find the closest point on the bounds to the circle center
        let closest_x = center.x.clamp(self.bounds.min_x, self.bounds.max_x);
        let closest_y = center.y.clamp(self.bounds.min_y, self.bounds.max_y);
        let closest_z = center.z.clamp(self.bounds.min_z, self.bounds.max_z);

        let distance = Self::distance(center, Position::new(closest_x, closest_y, closest_z));
        distance <= radius
    }

    /// Calculates distance between two positions
    fn distance(pos1: Position, pos2: Position) -> f32 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
    }
}

/// Object stored in the spatial index
#[derive(Debug, Clone)]
pub struct SpatialObject {
    /// Player ID
    pub player_id: PlayerId,
    /// Current position
    pub position: Position,
    /// Last update timestamp
    pub last_update: std::time::Instant,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl SpatialObject {
    /// Creates a new spatial object
    pub fn new(player_id: PlayerId, position: Position) -> Self {
        Self {
            player_id,
            position,
            last_update: std::time::Instant::now(),
            metadata: HashMap::new(),
        }
    }

    /// Updates the position of this object
    pub fn update_position(&mut self, new_position: Position) {
        self.position = new_position;
        self.last_update = std::time::Instant::now();
    }

    /// Adds metadata to this object
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
}

/// Regional quadtree for efficient spatial partitioning
#[derive(Debug)]
pub struct RegionQuadTree {
    /// Root node of the quadtree
    root: QuadTreeNode,
    /// Player ID to object mapping for fast updates
    player_objects: HashMap<PlayerId, SpatialObject>,
    /// Statistics for this quadtree
    stats: SpatialStats,
}

impl RegionQuadTree {
    /// Creates a new regional quadtree
    pub fn new(bounds: RegionBounds) -> Self {
        Self {
            root: QuadTreeNode::new(bounds, 0),
            player_objects: HashMap::new(),
            stats: SpatialStats::default(),
        }
    }

    /// Inserts or updates a player's position
    pub fn upsert_player(&mut self, player_id: PlayerId, position: Position) {
        // Remove existing entry if it exists
        if self.player_objects.contains_key(&player_id) {
            self.root.remove(player_id);
            self.stats.updates += 1;
        } else {
            self.stats.insertions += 1;
        }

        // Create new object and insert
        let object = SpatialObject::new(player_id, position);
        self.player_objects.insert(player_id, object.clone());
        self.root.insert(object);
    }

    /// Removes a player from the spatial index
    pub fn remove_player(&mut self, player_id: PlayerId) -> bool {
        if self.player_objects.remove(&player_id).is_some() {
            self.root.remove(player_id);
            self.stats.removals += 1;
            true
        } else {
            false
        }
    }

    /// Queries players within a radius of a position
    pub fn query_radius(&self, center: Position, radius: f32) -> Vec<QueryResult> {
        let mut spatial_objects = Vec::new();
        self.root.query_radius(center, radius, &mut spatial_objects);
        
        spatial_objects
            .into_iter()
            .map(|obj| {
                let distance = Self::distance(center, obj.position);
                QueryResult {
                    player_id: obj.player_id,
                    position: obj.position,
                    distance,
                    metadata: obj.metadata,
                }
            })
            .collect()
    }

    /// Performs a complex spatial query with filters
    pub fn query(&self, query: &SpatialQuery) -> Vec<QueryResult> {
        let mut results = self.query_radius(query.center, query.radius);
        
        // Apply filters
        if let Some(ref include_players) = query.filters.include_players {
            results.retain(|result| include_players.contains(&result.player_id));
        }
        
        if let Some(ref exclude_players) = query.filters.exclude_players {
            results.retain(|result| !exclude_players.contains(&result.player_id));
        }
        
        if let Some(min_distance) = query.filters.min_distance {
            results.retain(|result| result.distance >= min_distance);
        }
        
        // Sort by distance
        results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        
        // Limit results
        if let Some(max_results) = query.filters.max_results {
            results.truncate(max_results);
        }
        
        // Note: stats.queries update removed due to immutable reference
        results
    }

    /// Gets the current position of a player
    pub fn get_player_position(&self, player_id: PlayerId) -> Option<Position> {
        self.player_objects.get(&player_id).map(|obj| obj.position)
    }

    /// Gets all players within the spatial index
    pub fn get_all_players(&self) -> Vec<PlayerId> {
        self.player_objects.keys().copied().collect()
    }

    /// Gets statistics for this spatial index
    pub fn get_stats(&self) -> SpatialStats {
        self.stats.clone()
    }

    /// Calculates distance between two positions
    fn distance(pos1: Position, pos2: Position) -> f32 {
        let dx = pos1.x - pos2.x;
        let dy = pos1.y - pos2.y;
        let dz = pos1.z - pos2.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
    }
}

/// Main spatial partition manager
#[derive(Debug)]
pub struct SpatialPartition {
    /// Regional quadtrees indexed by region bounds
    regions: Arc<RwLock<HashMap<String, RegionQuadTree>>>,
    /// Player to region mapping
    player_regions: Arc<RwLock<HashMap<PlayerId, String>>>,
    /// Global spatial statistics
    stats: Arc<RwLock<GlobalSpatialStats>>,
}

impl SpatialPartition {
    /// Creates a new spatial partition system
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            player_regions: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(GlobalSpatialStats::default())),
        }
    }

    /// Adds a new region to the spatial partition
    pub async fn add_region(&self, region_name: String, bounds: RegionBounds) {
        let mut regions = self.regions.write().await;
        regions.insert(region_name.clone(), RegionQuadTree::new(bounds));
        
        let mut stats = self.stats.write().await;
        stats.total_regions += 1;
    }

    /// Removes a region from the spatial partition
    pub async fn remove_region(&self, region_name: &str) {
        let mut regions = self.regions.write().await;
        regions.remove(region_name);
        
        // Remove all players from this region
        let mut player_regions = self.player_regions.write().await;
        player_regions.retain(|_, region| region != region_name);
        
        let mut stats = self.stats.write().await;
        stats.total_regions = stats.total_regions.saturating_sub(1);
    }

    /// Updates a player's position in the spatial partition
    pub async fn update_player_position(
        &self,
        player_id: PlayerId,
        position: Position,
        region_name: String,
    ) {
        // Check if player needs to move between regions
        let mut player_regions = self.player_regions.write().await;
        let current_region = player_regions.get(&player_id).cloned();
        
        if current_region.as_ref() != Some(&region_name) {
            // Remove from old region if exists
            if let Some(old_region) = current_region {
                drop(player_regions);
                let mut regions = self.regions.write().await;
                if let Some(quadtree) = regions.get_mut(&old_region) {
                    quadtree.remove_player(player_id);
                }
                drop(regions);
                player_regions = self.player_regions.write().await;
            }
            
            // Update region mapping
            player_regions.insert(player_id, region_name.clone());
        }
        drop(player_regions);

        // Update position in the appropriate region
        let mut regions = self.regions.write().await;
        if let Some(quadtree) = regions.get_mut(&region_name) {
            quadtree.upsert_player(player_id, position);
        }
    }

    /// Removes a player from all spatial indices
    pub async fn remove_player(&self, player_id: PlayerId) {
        let mut player_regions = self.player_regions.write().await;
        if let Some(region_name) = player_regions.remove(&player_id) {
            drop(player_regions);
            
            let mut regions = self.regions.write().await;
            if let Some(quadtree) = regions.get_mut(&region_name) {
                quadtree.remove_player(player_id);
            }
        }
    }

    /// Performs a spatial query within a specific region
    pub async fn query_region(&self, region_name: &str, query: &SpatialQuery) -> Vec<QueryResult> {
        let regions = self.regions.read().await;
        if let Some(quadtree) = regions.get(region_name) {
            quadtree.query(query)
        } else {
            Vec::new()
        }
    }

    /// Performs a spatial query across all regions
    pub async fn query_all_regions(&self, query: &SpatialQuery) -> Vec<QueryResult> {
        let regions = self.regions.read().await;
        let mut all_results = Vec::new();
        
        for quadtree in regions.values() {
            let mut region_results = quadtree.query(query);
            all_results.append(&mut region_results);
        }
        
        // Sort by distance and apply global filters
        all_results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));
        
        if let Some(max_results) = query.filters.max_results {
            all_results.truncate(max_results);
        }
        
        all_results
    }

    /// Gets players near a position across regions
    pub async fn get_nearby_players(
        &self,
        position: Position,
        radius: f32,
        max_results: Option<usize>,
    ) -> Vec<QueryResult> {
        let query = SpatialQuery {
            center: position,
            radius,
            filters: QueryFilters {
                max_results,
                ..Default::default()
            },
        };
        
        self.query_all_regions(&query).await
    }

    /// Gets the region a player is currently in
    pub async fn get_player_region(&self, player_id: PlayerId) -> Option<String> {
        let player_regions = self.player_regions.read().await;
        player_regions.get(&player_id).cloned()
    }

    /// Gets global spatial statistics
    pub async fn get_global_stats(&self) -> GlobalSpatialStats {
        let mut stats = self.stats.write().await;
        
        // Update player count from regions
        let regions = self.regions.read().await;
        stats.total_players = regions.values()
            .map(|quadtree| quadtree.get_all_players().len())
            .sum();
        
        stats.clone()
    }
}

impl Default for SpatialPartition {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for a spatial index
#[derive(Debug, Clone, Default)]
pub struct SpatialStats {
    /// Number of insertions performed
    pub insertions: u64,
    /// Number of updates performed
    pub updates: u64,
    /// Number of removals performed
    pub removals: u64,
    /// Number of queries performed
    pub queries: u64,
    /// Average query time in microseconds
    pub avg_query_time_us: f64,
}

/// Global spatial statistics across all regions
#[derive(Debug, Clone, Default)]
pub struct GlobalSpatialStats {
    /// Total number of regions
    pub total_regions: usize,
    /// Total number of players across all regions
    pub total_players: usize,
    /// Total number of spatial queries performed
    pub total_queries: u64,
    /// Average players per region
    pub avg_players_per_region: f32,
    /// Memory usage in bytes (estimated)
    pub memory_usage_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quadtree_insertion() {
        let bounds = RegionBounds {
            min_x: 0.0, max_x: 1000.0,
            min_y: 0.0, max_y: 1000.0,
            min_z: 0.0, max_z: 1000.0,
        };
        
        let mut quadtree = RegionQuadTree::new(bounds);
        let player_id = PlayerId::new();
        let position = Position::new(100.0, 100.0, 100.0);
        
        quadtree.upsert_player(player_id, position);
        assert_eq!(quadtree.get_player_position(player_id), Some(position));
    }

    #[test]
    fn test_radius_query() {
        let bounds = RegionBounds {
            min_x: 0.0, max_x: 1000.0,
            min_y: 0.0, max_y: 1000.0,
            min_z: 0.0, max_z: 1000.0,
        };
        
        let mut quadtree = RegionQuadTree::new(bounds);
        
        // Add several players
        for i in 0..10 {
            let player_id = PlayerId::new();
            let position = Position::new(i as f64 * 50.0, 0.0, 0.0);
            quadtree.upsert_player(player_id, position);
        }
        
        // Query within radius
        let results = quadtree.query_radius(Position::new(100.0, 0.0, 0.0), 100.0);
        assert!(results.len() >= 2); // Should find players at 50.0 and 150.0
    }

    #[test]
    fn test_spatial_query_filters() {
        let bounds = RegionBounds {
            min_x: 0.0, max_x: 1000.0,
            min_y: 0.0, max_y: 1000.0,
            min_z: 0.0, max_z: 1000.0,
        };
        
        let mut quadtree = RegionQuadTree::new(bounds);
        let player_ids: Vec<PlayerId> = (0..5).map(|_| PlayerId::new()).collect();
        
        // Add players in a line
        for (i, &player_id) in player_ids.iter().enumerate() {
            let position = Position::new(i as f64 * 25.0, 0.0, 0.0);
            quadtree.upsert_player(player_id, position);
        }
        
        // Query with max results filter
        let query = SpatialQuery {
            center: Position::new(50.0, 0.0, 0.0),
            radius: 200.0,
            filters: QueryFilters {
                max_results: Some(2),
                ..Default::default()
            },
        };
        
        let results = quadtree.query(&query);
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_spatial_partition() {
        let partition = SpatialPartition::new();
        
        let bounds = RegionBounds {
            min_x: 0.0, max_x: 1000.0,
            min_y: 0.0, max_y: 1000.0,
            min_z: 0.0, max_z: 1000.0,
        };
        
        partition.add_region("test_region".to_string(), bounds).await;
        
        let player_id = PlayerId::new();
        let position = Position::new(100.0, 100.0, 100.0);
        
        partition.update_player_position(player_id, position, "test_region".to_string()).await;
        
        let player_region = partition.get_player_region(player_id).await;
        assert_eq!(player_region, Some("test_region".to_string()));
        
        let nearby = partition.get_nearby_players(position, 50.0, Some(10)).await;
        assert_eq!(nearby.len(), 1);
        assert_eq!(nearby[0].player_id, player_id);
    }
}