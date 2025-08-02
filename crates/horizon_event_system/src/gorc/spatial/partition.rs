/// Spatial partitioning system
use super::quadtree::RegionQuadTree;
use super::query::QueryResult;
use crate::types::{PlayerId, Position, Vec3};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Main spatial partitioning system
pub struct SpatialPartition {
    /// Regional quadtrees for different areas
    regions: Arc<RwLock<HashMap<String, RegionQuadTree>>>,
    /// Player to region mapping
    player_regions: Arc<RwLock<HashMap<PlayerId, String>>>,
}

impl SpatialPartition {
    /// Creates a new spatial partition system
    pub fn new() -> Self {
        Self {
            regions: Arc::new(RwLock::new(HashMap::new())),
            player_regions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds a region with specified bounds
    pub async fn add_region(&self, region_id: String, min: Vec3, max: Vec3) {
        let quadtree = RegionQuadTree::new(min, max);
        let mut regions = self.regions.write().await;
        regions.insert(region_id, quadtree);
    }

    /// Updates a player's position
    pub async fn update_player_position(&self, player_id: PlayerId, position: Position) {
        // Simplified: assume all players are in "default" region
        let region_id = "default".to_string();
        
        {
            let mut regions = self.regions.write().await;
            if let Some(region) = regions.get_mut(&region_id) {
                region.insert_player(player_id, position);
            }
        }

        {
            let mut player_regions = self.player_regions.write().await;
            player_regions.insert(player_id, region_id);
        }
    }

    /// Queries players within a radius
    pub async fn query_radius(&self, center: Position, radius: f32) -> Vec<QueryResult> {
        let mut regions = self.regions.write().await;
        let mut results = Vec::new();
        
        // Query all regions (simplified)
        for region in regions.values_mut() {
            results.extend(region.query_radius(center, radius));
        }
        
        results
    }

    /// Gets the total number of tracked players
    pub async fn player_count(&self) -> usize {
        let player_regions = self.player_regions.read().await;
        player_regions.len()
    }

    /// Gets the number of regions
    pub async fn region_count(&self) -> usize {
        let regions = self.regions.read().await;
        regions.len()
    }
}