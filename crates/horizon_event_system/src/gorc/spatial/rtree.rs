//! R*-tree based spatial indexing for GORC
//!
//! This module provides a 3D spatial index backed by the `rstar` crate. It replaces
//! the legacy quadtree implementation while preserving the existing public API
//! expected by the rest of the system.

use super::query::{QueryFilters, QueryResult, SpatialQuery};
use crate::types::{PlayerId, Position, Vec3};
use crate::utils::current_timestamp;
use rstar::{PointDistance, RTree, RTreeObject, AABB};
use std::collections::HashMap;

/// Entry stored inside the R-tree.
#[derive(Debug, Clone)]
struct SpatialEntry {
    object: SpatialObject,
    point: [f64; 3],
}

impl SpatialEntry {
    fn new(object: SpatialObject) -> Self {
        let point = [object.position.x, object.position.y, object.position.z];
        Self { object, point }
    }
}

impl PartialEq for SpatialEntry {
    fn eq(&self, other: &Self) -> bool {
        self.object.player_id == other.object.player_id
    }
}

impl Eq for SpatialEntry {}

impl RTreeObject for SpatialEntry {
    type Envelope = AABB<[f64; 3]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for SpatialEntry {
    fn distance_2(&self, point: &[f64; 3]) -> f64 {
        let dx = self.point[0] - point[0];
        let dy = self.point[1] - point[1];
        let dz = self.point[2] - point[2];
        dx * dx + dy * dy + dz * dz
    }

    fn contains_point(&self, point: &[f64; 3]) -> bool {
        (self.point[0] - point[0]).abs() < f64::EPSILON
            && (self.point[1] - point[1]).abs() < f64::EPSILON
            && (self.point[2] - point[2]).abs() < f64::EPSILON
    }
}

/// Object stored in the spatial index
#[derive(Debug, Clone)]
pub struct SpatialObject {
    /// Player identifier
    pub player_id: PlayerId,
    /// Object position
    pub position: Position,
    /// Last update timestamp
    pub last_updated: u64,
}

impl SpatialObject {
    /// Creates a new spatial object
    pub fn new(player_id: PlayerId, position: Position) -> Self {
        Self {
            player_id,
            position,
            last_updated: current_timestamp(),
        }
    }
}

/// Statistics for analyzing R-tree performance
#[derive(Debug, Clone, Default)]
pub struct SpatialIndexStats {
    pub total_insertions: usize,
    pub total_queries: usize,
    pub total_removals: usize,
    pub total_clears: usize,
    pub total_rebuilds: usize,
    pub last_query_result_count: usize,
    pub current_depth: u8,
    pub leaf_nodes: usize,
    pub internal_nodes: usize,
}

/// Detailed node statistics (approximated for R-tree)
#[derive(Debug, Clone, Default)]
pub struct NodeStats {
    pub total_objects: usize,
    pub max_depth: u8,
    pub leaf_nodes: usize,
    pub internal_nodes: usize,
}

/// High-performance regional R*-tree for efficient spatial queries
#[derive(Debug)]
pub struct RegionRTree {
    /// Root bounds of the tree (used for stats)
    bounds: (Vec3, Vec3),
    /// Underlying R-tree
    tree: RTree<SpatialEntry>,
    /// Cached entries for efficient updates/removals
    player_entries: HashMap<PlayerId, SpatialEntry>,
    /// Total objects in the tree
    object_count: usize,
    /// Performance statistics
    stats: SpatialIndexStats,
}

impl RegionRTree {
    /// Creates a new R-tree with specified bounds
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self {
            bounds: (min, max),
            tree: RTree::new(),
            player_entries: HashMap::new(),
            object_count: 0,
            stats: SpatialIndexStats::default(),
        }
    }

    /// Inserts or updates a player at a position with O(log n) performance
    pub fn insert_player(&mut self, player_id: PlayerId, position: Position) {
        let object = SpatialObject::new(player_id, position);
        self.insert_object(object);
    }

    /// Inserts or updates any spatial object with O(log n) performance
    pub fn insert_object(&mut self, object: SpatialObject) {
        let player_id = object.player_id;
        let entry = SpatialEntry::new(object);

        if let Some(existing) = self.player_entries.remove(&player_id) {
            let _ = self.tree.remove(&existing);
            self.object_count = self.object_count.saturating_sub(1);
        }

        self.tree.insert(entry.clone());
        self.player_entries.insert(player_id, entry);
        self.object_count += 1;
        self.stats.total_insertions += 1;
    }

    /// Queries players within a radius with O(log n) performance
    pub fn query_radius(&mut self, center: Position, radius: f64) -> Vec<QueryResult> {
        let query = SpatialQuery {
            center,
            radius,
            filters: QueryFilters::default(),
        };
        self.query(query)
    }

    /// Executes a spatial query with optional filters
    pub fn query(&mut self, query: SpatialQuery) -> Vec<QueryResult> {
        let center_point = [query.center.x, query.center.y, query.center.z];
        let radius_sq = query.radius * query.radius;

        let search_distance = radius_sq;

        let mut results: Vec<QueryResult> = self
            .tree
            .locate_within_distance(center_point, search_distance)
            .filter_map(|entry| {
                let object = &entry.object;

                // Apply include/exclude filters
                if let Some(include) = &query.filters.include_players {
                    if !include.contains(&object.player_id) {
                        return None;
                    }
                }

                if let Some(exclude) = &query.filters.exclude_players {
                    if exclude.contains(&object.player_id) {
                        return None;
                    }
                }

                let distance_sq = entry.distance_2(&center_point);
                if distance_sq > radius_sq {
                    return None;
                }

                let distance = distance_sq.sqrt();

                if let Some(min_distance) = query.filters.min_distance {
                    if distance < min_distance {
                        return None;
                    }
                }

                Some(QueryResult {
                    player_id: object.player_id,
                    position: object.position,
                    distance,
                    metadata: HashMap::new(),
                })
            })
            .collect();

        if let Some(max_results) = query.filters.max_results {
            results.truncate(max_results);
        }

        self.stats.total_queries += 1;
        self.stats.last_query_result_count = results.len();
        results
    }

    /// Removes all objects for a player (O(log n))
    pub fn remove_player(&mut self, player_id: PlayerId) -> usize {
        if let Some(existing) = self.player_entries.remove(&player_id) {
            let removed = self.tree.remove(&existing).is_some();
            if removed {
                self.object_count = self.object_count.saturating_sub(1);
                self.stats.total_removals += 1;
                return 1;
            }
        }
        0
    }

    /// Gets the total number of objects
    pub fn object_count(&self) -> usize {
        self.object_count
    }

    /// Checks whether a given player is indexed
    pub fn contains_player(&self, player_id: PlayerId) -> bool {
        self.player_entries.contains_key(&player_id)
    }

    /// Gets performance statistics
    pub fn get_stats(&mut self) -> SpatialIndexStats {
        let mut stats = self.stats.clone();
        let node_stats = self.get_node_stats();
        stats.current_depth = node_stats.max_depth;
        stats.leaf_nodes = node_stats.leaf_nodes;
        stats.internal_nodes = node_stats.internal_nodes;
        stats
    }

    /// Gets detailed tree structure statistics
    pub fn get_detailed_stats(&mut self) -> (SpatialIndexStats, NodeStats) {
        let stats = self.get_stats();
        let node_stats = self.get_node_stats();
        (stats, node_stats)
    }

    /// Estimates query efficiency (for monitoring)
    pub fn estimate_query_efficiency(&self, radius: f64) -> f64 {
        let total_volume = {
            let (min, max) = &self.bounds;
            (max.x - min.x).max(1.0)
                * (max.y - min.y).max(1.0)
                * (max.z - min.z).max(1.0)
        };
        let query_volume = (4.0 / 3.0) * std::f64::consts::PI * radius * radius * radius;
        let coverage_ratio = (query_volume / total_volume).min(1.0);
        1.0 - coverage_ratio
    }

    /// Clears all objects and resets the tree
    pub fn clear(&mut self) {
        let (min, max) = self.bounds;
        self.tree = RTree::new();
        self.player_entries.clear();
        self.object_count = 0;
        self.stats.total_clears += 1;
        self.bounds = (min, max);
    }

    /// Rebuilds the tree for better balance
    pub fn rebuild(&mut self) {
        let entries: Vec<_> = self.player_entries.values().cloned().collect();
        self.tree = RTree::bulk_load(entries);
        self.stats.total_rebuilds += 1;
    }

    /// Collects all objects from the tree
    pub fn collect_all_objects(&self) -> Vec<SpatialObject> {
        self.player_entries.values().map(|entry| entry.object.clone()).collect()
    }

    fn get_node_stats(&self) -> NodeStats {
        let leaf_nodes = self.tree.size();

        NodeStats {
            total_objects: self.object_count,
            max_depth: if leaf_nodes > 0 { 1 } else { 0 },
            leaf_nodes,
            internal_nodes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Vec3;

    #[test]
    fn test_insert_and_query() {
        let mut tree = RegionRTree::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 100.0),
        );

        let player_a = PlayerId::new();
        let player_b = PlayerId::new();

        tree.insert_player(player_a, Position::new(0.0, 0.0, 0.0));
        tree.insert_player(player_b, Position::new(50.0, 0.0, 0.0));

        assert_eq!(tree.object_count(), 2);
    assert!(tree.contains_player(player_a));
    assert!(tree.contains_player(player_b));

        let results = tree.query_radius(Position::new(0.0, 0.0, 0.0), 10.0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].player_id, player_a);

        let wider = tree.query_radius(Position::new(0.0, 0.0, 0.0), 60.0);
        let ids: Vec<PlayerId> = wider.iter().map(|r| r.player_id).collect();
        assert!(ids.contains(&player_a));
        assert!(ids.contains(&player_b));
        assert_eq!(wider.len(), 2);
    }

    #[test]
    fn test_update_player_position() {
        let mut tree = RegionRTree::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 100.0),
        );

        let player = PlayerId::new();
        tree.insert_player(player, Position::new(0.0, 0.0, 0.0));
        tree.insert_player(player, Position::new(20.0, 0.0, 0.0));

        let results = tree.query_radius(Position::new(0.0, 0.0, 0.0), 5.0);
        assert!(results.is_empty(), "Player should have moved out of range");

        let results = tree.query_radius(Position::new(20.0, 0.0, 0.0), 5.0);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].player_id, player);
    }

    #[test]
    fn test_remove_player() {
        let mut tree = RegionRTree::new(
            Vec3::new(-100.0, -100.0, -100.0),
            Vec3::new(100.0, 100.0, 100.0),
        );

        let player = PlayerId::new();
        tree.insert_player(player, Position::new(0.0, 0.0, 0.0));
        assert_eq!(tree.object_count(), 1);

        let removed = tree.remove_player(player);
        assert_eq!(removed, 1);
        assert_eq!(tree.object_count(), 0);
    }
}
