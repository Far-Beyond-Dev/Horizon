/// Spatial partitioning system
use super::query::{QueryResult, SpatialQuery};
use super::RegionRTree;
use crate::types::{PlayerId, Position, Vec3};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Main spatial partitioning system
#[derive(Debug)]
pub struct SpatialPartition {
    /// Regional spatial indexes for different areas
    regions: Arc<RwLock<HashMap<String, RegionRTree>>>,
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
        let mut regions = self.regions.write().await;
        regions
            .entry(region_id)
            .or_insert_with(|| RegionRTree::new(min, max));
    }

    /// Updates a player's position
    pub async fn update_player_position(&self, player_id: PlayerId, position: Position) {
        // Simplified: assume all players are in "default" region
        let region_id = "default".to_string();
        
        let mut regions = self.regions.write().await;
        let region = regions.entry(region_id.clone()).or_insert_with(|| {
            // Default region bounds (large enough for most worlds)
            RegionRTree::new(
                Vec3::new(-10_000.0, -10_000.0, -1_000.0),
                Vec3::new(10_000.0, 10_000.0, 1_000.0),
            )
        });

        region.insert_player(player_id, position);
        drop(regions);

        {
            let mut player_regions = self.player_regions.write().await;
            player_regions.insert(player_id, region_id);
        }
    }

    /// Queries players within a radius
    pub async fn query_radius(&self, center: Position, radius: f64) -> Vec<QueryResult> {
        let mut regions = self.regions.write().await;
        let mut results = Vec::new();
        
        // Query all regions (simplified)
        for region in regions.values_mut() {
            results.extend(region.query_radius(center, radius.into()));
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

    /// Removes a player from the spatial partition
    pub async fn remove_player(&self, player_id: PlayerId) {
        let region_id = {
            let mut player_regions = self.player_regions.write().await;
            player_regions.remove(&player_id)
        };

        if let Some(region_id) = region_id {
            let mut regions = self.regions.write().await;
            if let Some(region) = regions.get_mut(&region_id) {
                region.remove_player(player_id);
            }
        }
    }

    /// Runs a spatial query with filters
    pub async fn query(&self, query: SpatialQuery) -> Vec<QueryResult> {
        let mut regions = self.regions.write().await;
        let mut results = Vec::new();

        for region in regions.values_mut() {
            results.extend(region.query(query.clone()))
        }

        results
    }
}