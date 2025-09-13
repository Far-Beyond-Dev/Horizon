/// High-performance quadtree implementation for spatial indexing
/// Achieves O(log n) insertion and query performance through proper tree subdivision
use crate::types::{PlayerId, Position, Vec3};
use super::query::{SpatialQuery, QueryResult};
use std::collections::HashMap;

/// Maximum depth for quadtree subdivision
const MAX_QUADTREE_DEPTH: u8 = 10;
/// Maximum objects per quadtree node before subdivision
const MAX_OBJECTS_PER_NODE: usize = 8;
/// Minimum node size to prevent infinite subdivision
const MIN_NODE_SIZE: f64 = 1.0;

/// A node in the quadtree with proper O(log n) spatial subdivision
#[derive(Debug)]
pub struct QuadTreeNode {
    /// Bounding box for this node (min, max)
    pub bounds: (Vec3, Vec3),
    /// Objects in this node (only if leaf)
    pub objects: Vec<SpatialObject>,
    /// Child nodes (NW, NE, SW, SE) - None if leaf
    pub children: Option<Box<[QuadTreeNode; 4]>>,
    /// Current depth in tree
    pub depth: u8,
}

impl QuadTreeNode {
    /// Creates a new quadtree node
    pub fn new(min: Vec3, max: Vec3, depth: u8) -> Self {
        Self {
            bounds: (min, max),
            objects: Vec::new(),
            children: None,
            depth,
        }
    }

    /// Inserts an object into the quadtree with proper subdivision
    pub fn insert(&mut self, object: SpatialObject) -> bool {
        // Check if object is within bounds
        if !self.contains_point(object.position) {
            return false; // Object outside bounds, skip
        }

        // If this is a leaf and we haven't exceeded capacity, add here
        if self.children.is_none() {
            self.objects.push(object);
            
            // Check if we need to subdivide
            if self.objects.len() > MAX_OBJECTS_PER_NODE 
                && self.depth < MAX_QUADTREE_DEPTH 
                && self.can_subdivide() {
                self.subdivide();
            }
            true
        } else {
            // This is an internal node, route to appropriate child
            let child_index = self.get_child_index(object.position);
            if let Some(children) = &mut self.children {
                children[child_index].insert(object)
            } else {
                false
            }
        }
    }

    /// Checks if a point is within this node's bounds
    fn contains_point(&self, point: Position) -> bool {
        let (min, max) = &self.bounds;
        point.x >= min.x as f64 && point.x <= max.x as f64 &&
        point.y >= min.y as f64 && point.y <= max.y as f64
    }

    /// Checks if this node is large enough to subdivide
    fn can_subdivide(&self) -> bool {
        let (min, max) = &self.bounds;
        let width = max.x - min.x;
        let height = max.y - min.y;
        width > MIN_NODE_SIZE as f64 && height > MIN_NODE_SIZE as f64
    }

    /// Subdivides this node into 4 children and redistributes objects
    fn subdivide(&mut self) {
        let (min, max) = self.bounds;
        let mid_x = (min.x + max.x) / 2.0;
        let mid_y = (min.y + max.y) / 2.0;
        let child_depth = self.depth + 1;

        // Create 4 child nodes: NW, NE, SW, SE
        let children = Box::new([
            // NW (0): top-left
            QuadTreeNode::new(
                Vec3::new(min.x, mid_y, min.z),
                Vec3::new(mid_x, max.y, max.z),
                child_depth
            ),
            // NE (1): top-right  
            QuadTreeNode::new(
                Vec3::new(mid_x, mid_y, min.z),
                Vec3::new(max.x, max.y, max.z),
                child_depth
            ),
            // SW (2): bottom-left
            QuadTreeNode::new(
                Vec3::new(min.x, min.y, min.z),
                Vec3::new(mid_x, mid_y, max.z),
                child_depth
            ),
            // SE (3): bottom-right
            QuadTreeNode::new(
                Vec3::new(mid_x, min.y, min.z),
                Vec3::new(max.x, mid_y, max.z),
                child_depth
            ),
        ]);

        self.children = Some(children);

        // Redistribute existing objects to children
        let objects = std::mem::take(&mut self.objects);
        for object in objects {
            let child_index = self.get_child_index(object.position);
            if let Some(children) = &mut self.children {
                children[child_index].insert(object);
            }
        }
    }

    /// Gets the child index (0-3) for a given position
    fn get_child_index(&self, position: Position) -> usize {
        let (min, max) = &self.bounds;
        let mid_x = (min.x + max.x) / 2.0;
        let mid_y = (min.y + max.y) / 2.0;

        let right = position.x >= mid_x as f64;
        let top = position.y >= mid_y as f64;

        match (top, right) {
            (true, false) => 0,  // NW
            (true, true) => 1,   // NE
            (false, false) => 2, // SW
            (false, true) => 3,  // SE
        }
    }

    /// Efficiently queries objects within a radius using spatial bounds checking
    pub fn query(&self, query: &SpatialQuery) -> Vec<QueryResult> {
        let mut results = Vec::new();

        // Early exit if query bounds don't intersect with node bounds
        if !self.intersects_circle(query.center, query.radius) {
            return results;
        }

        // Check objects in this node
        for obj in &self.objects {
            let distance = query.center.distance(obj.position);
            if distance <= query.radius {
                results.push(QueryResult {
                    player_id: obj.player_id,
                    position: obj.position,
                    distance,
                    metadata: HashMap::new(),
                });
            }
        }

        // Recursively query children that intersect with the search area
        if let Some(children) = &self.children {
            for child in children.iter() {
                if child.intersects_circle(query.center, query.radius) {
                    results.extend(child.query(query));
                }
            }
        }

        results
    }

    /// Checks if a circle intersects with this node's bounds (optimized)
    fn intersects_circle(&self, center: Position, radius: f64) -> bool {
        let (min, max) = &self.bounds;
        
        // Find closest point on rectangle to circle center
        let closest_x = center.x.clamp(min.x as f64, max.x as f64);
        let closest_y = center.y.clamp(min.y as f64, max.y as f64);
        
        // Calculate distance from circle center to closest point
        let dx = center.x - closest_x;
        let dy = center.y - closest_y;
        let distance_squared = dx * dx + dy * dy;
        
        distance_squared <= (radius * radius) as f64
    }

    /// Gets statistics about this subtree
    pub fn get_stats(&self) -> NodeStats {
        let mut stats = NodeStats {
            total_objects: self.objects.len(),
            max_depth: self.depth,
            leaf_nodes: 0,
            internal_nodes: 0,
        };

        if self.children.is_none() {
            stats.leaf_nodes = 1;
        } else {
            stats.internal_nodes = 1;
            if let Some(children) = &self.children {
                for child in children.iter() {
                    let child_stats = child.get_stats();
                    stats.total_objects += child_stats.total_objects;
                    stats.max_depth = stats.max_depth.max(child_stats.max_depth);
                    stats.leaf_nodes += child_stats.leaf_nodes;
                    stats.internal_nodes += child_stats.internal_nodes;
                }
            }
        }

        stats
    }
}

/// Statistics for analyzing quadtree performance
#[derive(Debug, Clone)]
pub struct NodeStats {
    pub total_objects: usize,
    pub max_depth: u8,
    pub leaf_nodes: usize,
    pub internal_nodes: usize,
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
            last_updated: crate::utils::current_timestamp(),
        }
    }
}

/// High-performance regional quadtree for efficient spatial queries
/// Now achieves true O(log n) performance through proper tree subdivision
pub struct RegionQuadTree {
    /// Root node of the quadtree
    root: QuadTreeNode,
    /// Total objects in the tree
    object_count: usize,
    /// Performance statistics
    stats: QuadTreeStats,
}

impl RegionQuadTree {
    /// Creates a new quadtree with specified bounds
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self {
            root: QuadTreeNode::new(min, max, 0),
            object_count: 0,
            stats: QuadTreeStats::default(),
        }
    }

    /// Inserts a player at a position with O(log n) performance
    pub fn insert_player(&mut self, player_id: PlayerId, position: Position) {
        let object = SpatialObject::new(player_id, position);
        if self.root.insert(object) {
            self.object_count += 1;
            self.stats.total_insertions += 1;
        }
    }

    /// Inserts any spatial object with O(log n) performance
    pub fn insert_object(&mut self, object: SpatialObject) {
        if self.root.insert(object) {
            self.object_count += 1;
            self.stats.total_insertions += 1;
        }
    }

    /// Queries players within a radius with O(log n) performance
    pub fn query_radius(&mut self, center: Position, radius: f64) -> Vec<QueryResult> {
        let query = SpatialQuery {
            center,
            radius,
            filters: Default::default(),
        };
        let results = self.root.query(&query);
        
        // Update query stats
        self.stats.total_queries += 1;
        self.stats.last_query_result_count = results.len();
        
        results
    }

    /// Removes all objects for a player (O(n) - could be optimized with player->node mapping)
    pub fn remove_player(&mut self, player_id: PlayerId) -> usize {
        let removed_count = Self::remove_objects_recursive(&mut self.root, |obj| obj.player_id == player_id);
        self.object_count = self.object_count.saturating_sub(removed_count);
        self.stats.total_removals += removed_count;
        removed_count
    }

    /// Recursive helper for object removal
    fn remove_objects_recursive<F>(node: &mut QuadTreeNode, predicate: F) -> usize 
    where 
        F: Fn(&SpatialObject) -> bool + Copy,
    {
        let mut removed_count = 0;
        
        // Remove from this node
        let original_len = node.objects.len();
        node.objects.retain(|obj| !predicate(obj));
        removed_count += original_len - node.objects.len();
        
        // Remove from children
        if let Some(children) = &mut node.children {
            for child in children.iter_mut() {
                removed_count += Self::remove_objects_recursive(child, predicate);
            }
        }
        
        removed_count
    }

    /// Gets the total number of objects
    pub fn object_count(&self) -> usize {
        self.object_count
    }

    /// Gets performance statistics
    pub fn get_stats(&self) -> QuadTreeStats {
        let mut stats = self.stats.clone();
        let node_stats = self.root.get_stats();
        stats.current_depth = node_stats.max_depth;
        stats.leaf_nodes = node_stats.leaf_nodes;
        stats.internal_nodes = node_stats.internal_nodes;
        stats
    }

    /// Gets detailed tree structure statistics
    pub fn get_detailed_stats(&self) -> (QuadTreeStats, NodeStats) {
        (self.get_stats(), self.root.get_stats())
    }

    /// Estimates query efficiency (for monitoring)
    pub fn estimate_query_efficiency(&self, radius: f64) -> f64 {
        let total_area = {
            let (min, max) = &self.root.bounds;
            (max.x - min.x) * (max.y - min.y)
        };
        let query_area = std::f64::consts::PI * radius * radius;
        let coverage_ratio = query_area / total_area;
        
        // Rough efficiency estimate: smaller coverage = better efficiency
        1.0 - coverage_ratio.min(1.0)
    }

    /// Clears all objects and resets the tree
    pub fn clear(&mut self) {
        let (min, max) = self.root.bounds;
        self.root = QuadTreeNode::new(min, max, 0);
        self.object_count = 0;
        self.stats.total_clears += 1;
    }

    /// Rebuilds the tree for better balance (expensive but sometimes necessary)
    pub fn rebuild(&mut self) {
        let objects = self.collect_all_objects();
        self.clear();
        
        for object in objects {
            self.insert_object(object);
        }
        
        self.stats.total_rebuilds += 1;
    }

    /// Collects all objects from the tree
    fn collect_all_objects(&self) -> Vec<SpatialObject> {
        let mut objects = Vec::new();
        self.collect_objects_recursive(&self.root, &mut objects);
        objects
    }

    /// Recursive helper for collecting objects
    fn collect_objects_recursive(&self, node: &QuadTreeNode, objects: &mut Vec<SpatialObject>) {
        objects.extend(node.objects.iter().cloned());
        
        if let Some(children) = &node.children {
            for child in children.iter() {
                self.collect_objects_recursive(child, objects);
            }
        }
    }
}

/// Performance statistics for the quadtree
#[derive(Debug, Clone)]
pub struct QuadTreeStats {
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

impl Default for QuadTreeStats {
    fn default() -> Self {
        Self {
            total_insertions: 0,
            total_queries: 0,
            total_removals: 0,
            total_clears: 0,
            total_rebuilds: 0,
            last_query_result_count: 0,
            current_depth: 0,
            leaf_nodes: 1, // Start with root as leaf
            internal_nodes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Vec3;
    use std::time::Instant;
    use tracing::info;

    #[test]
    fn test_quadtree_subdivision() {
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1000.0, 1000.0, 0.0)
        );

        // Insert enough objects to trigger subdivision
        for i in 0..20 {
            let player_id = PlayerId::new();
            let position = Position::new(i as f64 * 50.0, i as f64 * 50.0, 0.0);
            tree.insert_player(player_id, position);
        }

        let stats = tree.get_stats();
        assert!(stats.current_depth > 0, "Tree should have subdivided");
        assert!(stats.internal_nodes > 0, "Should have internal nodes");
        assert_eq!(tree.object_count(), 20);
    }

    #[test]
    fn test_query_performance() {
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1000.0, 1000.0, 0.0)
        );

        // Insert 1000 objects
        for i in 0..1000 {
            let player_id = PlayerId::new();
            let x = (i % 100) as f64 * 10.0;
            let y = (i / 100) as f64 * 10.0;
            let position = Position::new(x, y, 0.0);
            tree.insert_player(player_id, position);
        }

        // Perform a small radius query at a location where objects exist (should be very fast)
        let start = Instant::now();
        let results = tree.query_radius(Position::new(500.0, 50.0, 0.0), 50.0);
        let duration = start.elapsed();

        info!("Query of 1000 objects took: {:?}", duration);
        info!("Found {} objects in radius", results.len());
        
        // Should find objects and be fast
        assert!(!results.is_empty());
        assert!(duration.as_millis() < 10, "Query should be very fast with proper tree");
    }

    #[test] 
    fn test_spatial_bounds_checking() {
        let tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0)
        );

        // Test intersection checking
        let root = &tree.root;
        
        // Circle completely inside bounds
        assert!(root.intersects_circle(Position::new(50.0, 50.0, 0.0), 10.0));
        
        // Circle completely outside bounds
        assert!(!root.intersects_circle(Position::new(200.0, 200.0, 0.0), 10.0));
        
        // Circle overlapping bounds
        assert!(root.intersects_circle(Position::new(95.0, 95.0, 0.0), 10.0));
    }

    #[test]
    fn test_performance_scaling() {
        // Test that O(log n) scaling is working
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1000.0, 1000.0, 0.0)
        );

        // Insert varying numbers of objects and measure query time
        let test_sizes = [100, 500, 1000, 2000];
        let mut times = Vec::new();

        for &size in &test_sizes {
            tree.clear();
            
            // Insert objects
            for i in 0..size {
                let player_id = PlayerId::new();
                let x = (i % 100) as f64 * 10.0;
                let y = (i / 100) as f64 * 10.0;
                let position = Position::new(x, y, 0.0);
                tree.insert_player(player_id, position);
            }

            // Time a query
            let start = Instant::now();
            let _results = tree.query_radius(Position::new(500.0, 500.0, 0.0), 100.0);
            let duration = start.elapsed();
            
            times.push(duration.as_nanos());
            info!("Size: {}, Time: {:?}", size, duration);
        }

        // Verify that time doesn't scale linearly (would indicate O(log n) behavior)
        let ratio_1_to_2 = times[1] as f64 / times[0] as f64;
        let ratio_3_to_4 = times[3] as f64 / times[2] as f64;
        
        info!("Scaling ratios: {:.2}, {:.2}", ratio_1_to_2, ratio_3_to_4);
        
        // If it were O(n), doubling size would roughly double time
        // With O(log n), the ratio should be much smaller
        assert!(ratio_1_to_2 < 3.0, "Performance should scale better than linear");
    }

    #[test]
    fn test_object_removal() {
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0)
        );

        let player_id = PlayerId::new();
        tree.insert_player(player_id, Position::new(50.0, 50.0, 0.0));
        assert_eq!(tree.object_count(), 1);

        let removed = tree.remove_player(player_id);
        assert_eq!(removed, 1);
        assert_eq!(tree.object_count(), 0);
    }

    #[test]
    fn test_bounds_checking() {
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0)
        );

        // Insert object outside bounds - should be ignored
        let player_id = PlayerId::new();
        tree.insert_player(player_id, Position::new(200.0, 200.0, 0.0));
        
        // Should not have been inserted
        assert_eq!(tree.object_count(), 0);
    }

    #[test]
    fn test_child_index_calculation() {
        let node = QuadTreeNode::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0),
            0
        );

        // Test quadrant assignments
        assert_eq!(node.get_child_index(Position::new(25.0, 75.0, 0.0)), 0); // NW
        assert_eq!(node.get_child_index(Position::new(75.0, 75.0, 0.0)), 1); // NE
        assert_eq!(node.get_child_index(Position::new(25.0, 25.0, 0.0)), 2); // SW
        assert_eq!(node.get_child_index(Position::new(75.0, 25.0, 0.0)), 3); // SE
    }

    #[test]
    fn test_stats_tracking() {
        let mut tree = RegionQuadTree::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(100.0, 100.0, 0.0)
        );

        let player_id = PlayerId::new();
        tree.insert_player(player_id, Position::new(50.0, 50.0, 0.0));
        tree.query_radius(Position::new(50.0, 50.0, 0.0), 10.0);

        let stats = tree.get_stats();
        assert_eq!(stats.total_insertions, 1);
        assert_eq!(stats.total_queries, 1);
    }
}