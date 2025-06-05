use anyhow::{Context, Result};
use dashmap::DashMap;
use nalgebra::{Point3, Vector3};
use rstar::{RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug, instrument};
use uuid::Uuid;

use crate::config::WorldConfig;

/// World manager handles spatial partitioning and region management
pub struct WorldManager {
    config: WorldConfig,
    regions: Arc<DashMap<Uuid, WorldRegion>>,
    spatial_index: Arc<RwLock<RTree<SpatialRegion>>>,
    objects: Arc<DashMap<Uuid, WorldObject>>,
    object_spatial_index: Arc<RwLock<RTree<SpatialObject>>>,
    region_assignments: Arc<DashMap<Uuid, Uuid>>, // object_id -> region_id
    simulation_handle: Option<tokio::task::JoinHandle<()>>,
}

/// World region represents a spatial area of the game world
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRegion {
    pub id: Uuid,
    pub name: String,
    pub center: Point3<f64>,
    pub size: f64,
    pub bounds: AABB<[f64; 3]>,
    pub object_count: usize,
    pub max_objects: usize,
    pub region_type: RegionType,
    pub properties: RegionProperties,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub active: bool,
    pub server_assignment: Option<Uuid>, // Which server owns this region
}

/// Types of regions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegionType {
    Normal,
    Safe,
    PvP,
    Instance,
    Dungeon,
    Custom(String),
}

/// Region properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionProperties {
    pub physics_enabled: bool,
    pub simulation_rate: f64,
    pub max_players: usize,
    pub level_range: Option<(u32, u32)>,
    pub environment: String,
    pub weather: Option<WeatherConfig>,
    pub custom_data: serde_json::Value,
}

/// Weather configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    pub weather_type: String,
    pub intensity: f64,
    pub duration: Option<std::time::Duration>,
    pub effects: Vec<String>,
}

/// Spatial region for R-tree indexing
#[derive(Debug, Clone)]
pub struct SpatialRegion {
    pub id: Uuid,
    pub bounds: AABB<[f64; 3]>,
}

impl RTreeObject for SpatialRegion {
    type Envelope = AABB<[f64; 3]>;
    
    fn envelope(&self) -> Self::Envelope {
        self.bounds
    }
}

/// World object represents any entity in the game world
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldObject {
    pub id: Uuid,
    pub object_type: ObjectType,
    pub position: Point3<f64>,
    pub rotation: Vector3<f64>,
    pub scale: Vector3<f64>,
    pub velocity: Vector3<f64>,
    pub properties: ObjectProperties,
    pub region_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
    pub active: bool,
}

/// Types of objects
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectType {
    Player,
    NPC,
    Vehicle,
    Building,
    Item,
    Projectile,
    Effect,
    Trigger,
    Custom(String),
}

/// Object properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectProperties {
    pub health: Option<f64>,
    pub max_health: Option<f64>,
    pub mass: Option<f64>,
    pub collision_enabled: bool,
    pub visible: bool,
    pub interactable: bool,
    pub persistent: bool,
    pub tags: Vec<String>,
    pub custom_data: serde_json::Value,
}

/// Spatial object for R-tree indexing
#[derive(Debug, Clone)]
pub struct SpatialObject {
    pub id: Uuid,
    pub position: Point3<f64>,
    pub bounds: AABB<[f64; 3]>,
    pub object_type: ObjectType,
}

impl RTreeObject for SpatialObject {
    type Envelope = AABB<[f64; 3]>;
    
    fn envelope(&self) -> Self::Envelope {
        self.bounds
    }
}

/// World statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldStats {
    pub total_regions: usize,
    pub total_objects: usize,
    pub active_players: usize,
    pub memory_usage_mb: u64,
    pub simulation_fps: f64,
    pub region_assignments: std::collections::HashMap<Uuid, Uuid>, // region_id -> server_id
}

/// Region info for listing
#[derive(Debug, Clone)]
pub struct RegionInfo {
    pub name: String,
    pub size: f64,
    pub object_count: usize,
    pub active: bool,
    pub region_type: RegionType,
}

/// Object query parameters
#[derive(Debug, Clone)]
pub struct ObjectQuery {
    pub center: Point3<f64>,
    pub radius: f64,
    pub object_types: Option<Vec<ObjectType>>,
    pub tags: Option<Vec<String>>,
    pub limit: Option<usize>,
}

/// Object update batch for efficient updates
#[derive(Debug, Clone)]
pub struct ObjectUpdateBatch {
    pub updates: Vec<ObjectUpdate>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Single object update
#[derive(Debug, Clone)]
pub struct ObjectUpdate {
    pub object_id: Uuid,
    pub position: Option<Point3<f64>>,
    pub rotation: Option<Vector3<f64>>,
    pub velocity: Option<Vector3<f64>>,
    pub properties: Option<ObjectProperties>,
}

impl WorldManager {
    /// Create new world manager
    #[instrument(skip(config))]
    pub async fn new(config: &WorldConfig) -> Result<Self> {
        info!("🌍 Initializing world manager");
        
        let manager = Self {
            config: config.clone(),
            regions: Arc::new(DashMap::new()),
            spatial_index: Arc::new(RwLock::new(RTree::new())),
            objects: Arc::new(DashMap::new()),
            object_spatial_index: Arc::new(RwLock::new(RTree::new())),
            region_assignments: Arc::new(DashMap::new()),
            simulation_handle: None,
        };
        
        // Load existing regions from storage
        manager.load_regions_from_storage().await?;
        
        // Load existing objects from storage
        manager.load_objects_from_storage().await?;
        
        info!("✅ World manager initialized with {} regions and {} objects", 
              manager.regions.len(), manager.objects.len());
        Ok(manager)
    }
    
    /// Load regions from storage
    async fn load_regions_from_storage(&self) -> Result<()> {
        debug!("Loading regions from storage");
        
        // Implementation would load from database
        // For now, create a default region if none exist
        if self.regions.is_empty() {
            self.create_default_region().await?;
        }
        
        Ok(())
    }
    
    /// Load objects from storage
    async fn load_objects_from_storage(&self) -> Result<()> {
        debug!("Loading objects from storage");
        
        // Implementation would load from database
        // Objects would be loaded lazily as regions become active
        
        Ok(())
    }
    
    /// Create default region
    async fn create_default_region(&self) -> Result<()> {
        let region_id = Uuid::new_v4();
        let center = Point3::new(0.0, 0.0, 0.0);
        let size = self.config.default_region_size;
        
        let region = WorldRegion {
            id: region_id,
            name: "Default Region".to_string(),
            center,
            size,
            bounds: AABB::from_corners(
                [center.x - size/2.0, center.y - size/2.0, center.z - size/2.0],
                [center.x + size/2.0, center.y + size/2.0, center.z + size/2.0],
            ),
            object_count: 0,
            max_objects: self.config.max_objects_per_region,
            region_type: RegionType::Normal,
            properties: RegionProperties {
                physics_enabled: self.config.physics_enabled,
                simulation_rate: self.config.simulation_tick_rate as f64,
                max_players: 1000,
                level_range: None,
                environment: "default".to_string(),
                weather: None,
                custom_data: serde_json::Value::Null,
            },
            created_at: chrono::Utc::now(),
            last_updated: chrono::Utc::now(),
            active: true,
            server_assignment: None,
        };
        
        self.add_region(region).await?;
        info!("🏞️  Created default region: {}", region_id);
        
        Ok(())
    }
    
    /// Create a new region
    #[instrument(skip(self))]
    pub async fn create_region(&self, name: &str, size: f64) -> Result<Uuid> {
        let region_id = Uuid::new_v4();
        
        // Find a good location for the new region (simplified)
        let center = self.find_free_location(size).await?;
        
        let region = WorldRegion {
            id: region_id,
            name: name.to_string(),
            center,
            size,
            bounds: AABB::from_corners(
                [center.x - size/2.0, center.y - size/2.0, center.z - size/2.0],
                [center.x + size/2.0, center.y + size/2.0, center.z + size/2.0],
            ),
            object_count: 0,
            max_objects: self.config.max_objects_per_region,
            region_type: RegionType::Normal,
            properties: RegionProperties {
                physics_enabled: self.config.physics_enabled,
                simulation_rate: self.config.simulation_tick_rate as f64,
                max_players: 1000,
                level_range: None,
                environment: "default".to_string(),
                weather: None,
                custom_data: serde_json::Value::Null,
            },
            created_at: chrono::Utc::now(),
            last_updated: chrono::Utc::now(),
            active: true,
            server_assignment: None,
        };
        
        self.add_region(region).await?;
        
        info!("🏞️  Created region '{}' with ID: {}", name, region_id);
        Ok(region_id)
    }
    
    /// Add a region to the world
    async fn add_region(&self, region: WorldRegion) -> Result<()> {
        let spatial_region = SpatialRegion {
            id: region.id,
            bounds: region.bounds,
        };
        
        // Add to spatial index
        {
            let mut spatial_index = self.spatial_index.write().await;
            spatial_index.insert(spatial_region);
        }
        
        // Store region
        self.regions.insert(region.id, region.clone());
        
        // Persist to storage
        println!("TODO!!!!! Save region {} to storage", region.id);
        
        Ok(())
    }
    
    /// Find a free location for a new region
    async fn find_free_location(&self, size: f64) -> Result<Point3<f64>> {
        // Simplified implementation - just offset from existing regions
        let existing_count = self.regions.len() as f64;
        let offset = existing_count * size * 1.5;
        
        Ok(Point3::new(offset, 0.0, 0.0))
    }
    
    /// Add an object to the world
    #[instrument(skip(self, object))]
    pub async fn add_object(&self, mut object: WorldObject) -> Result<()> {
        // Find the appropriate region for this object
        let region_id = self.find_region_for_position(&object.position).await?;
        object.region_id = Some(region_id);
        
        // Create spatial object for indexing
        let spatial_object = SpatialObject {
            id: object.id,
            position: object.position,
            bounds: self.calculate_object_bounds(&object),
            object_type: object.object_type.clone(),
        };
        
        // Add to spatial index
        {
            let mut spatial_index = self.object_spatial_index.write().await;
            spatial_index.insert(spatial_object);
        }
        
        // Update region object count
        if let Some(mut region) = self.regions.get_mut(&region_id) {
            region.object_count += 1;
            region.last_updated = chrono::Utc::now();
        }
        
        // Store object
        self.objects.insert(object.id, object.clone());
        self.region_assignments.insert(object.id, region_id);
        
        // Persist to storage if persistent
        if object.properties.persistent {
            todo!("Save object {} to storage", object.id);
        }
        
        debug!("➕ Added object {} to region {}", object.id, region_id);
        Ok(())
    }
    
    /// Remove an object from the world
    #[instrument(skip(self))]
    pub async fn remove_object(&self, object_id: Uuid) -> Result<()> {
        if let Some((_, object)) = self.objects.remove(&object_id) {
            // Remove from spatial index
            {
                let mut spatial_index = self.object_spatial_index.write().await;
                // Note: rstar doesn't have direct removal, so we'd need to rebuild
                // In production, consider using a different spatial index or 
                // mark objects as inactive instead of removing
            }
            
            // Update region object count
            if let Some(region_id) = object.region_id {
                if let Some(mut region) = self.regions.get_mut(&region_id) {
                    region.object_count = region.object_count.saturating_sub(1);
                    region.last_updated = chrono::Utc::now();
                }
                self.region_assignments.remove(&object_id);
            }
            
            // Remove from storage
            println!("TODO!!!!! Remove object {} from storage", object_id);
            
            debug!("➖ Removed object {}", object_id);
        }
        
        Ok(())
    }
    
    /// Update an object
    #[instrument(skip(self, update))]
    pub async fn update_object(&self, object_id: Uuid, update: ObjectUpdate) -> Result<()> {
        if let Some(mut object_ref) = self.objects.get_mut(&object_id) {
            let mut moved = false;
            
            // Apply updates
            if let Some(position) = update.position {
                object_ref.position = position;
                moved = true;
            }
            
            if let Some(rotation) = update.rotation {
                object_ref.rotation = rotation;
            }
            
            if let Some(velocity) = update.velocity {
                object_ref.velocity = velocity;
            }
            
            if let Some(properties) = update.properties {
                object_ref.properties = properties;
            }
            
            object_ref.last_updated = chrono::Utc::now();
            
            // If object moved, check if it needs to change regions
            if moved {
                let current_region_id = object_ref.region_id;
                let new_region_id = self.find_region_for_position(&object_ref.position).await?;
                
                if current_region_id != Some(new_region_id) {
                    // Move object to new region
                    self.move_object_to_region(object_id, new_region_id).await?;
                }
                
                // Update spatial index
                self.update_object_spatial_index(object_id, &*object_ref).await?;
            }
            
            // Persist to storage if persistent
            if object_ref.properties.persistent {
                println!("TODO!!!!! Save updated object {} to storage", object_id);
            }
        }
        
        Ok(())
    }
    
    /// Update multiple objects in a batch
    #[instrument(skip(self, batch))]
    pub async fn update_objects_batch(&self, batch: ObjectUpdateBatch) -> Result<()> {
        for update in batch.updates {
            let object_id = update.object_id;
            if let Err(e) = self.update_object(object_id, update).await {
                warn!("Failed to update object {}: {}", object_id, e);
            }
        }
        
        Ok(())
    }
    
    /// Move object to a different region
    async fn move_object_to_region(&self, object_id: Uuid, new_region_id: Uuid) -> Result<()> {
        // Update region object counts
        if let Some(old_region_id) = self.region_assignments.get(&object_id).map(|r| *r) {
            if let Some(mut region) = self.regions.get_mut(&old_region_id) {
                region.object_count = region.object_count.saturating_sub(1);
                region.last_updated = chrono::Utc::now();
            }
        }
        
        if let Some(mut region) = self.regions.get_mut(&new_region_id) {
            region.object_count += 1;
            region.last_updated = chrono::Utc::now();
        }
        
        // Update object's region assignment
        if let Some(mut object) = self.objects.get_mut(&object_id) {
            object.region_id = Some(new_region_id);
        }
        
        self.region_assignments.insert(object_id, new_region_id);
        
        debug!("📦 Moved object {} to region {}", object_id, new_region_id);
        Ok(())
    }
    
    /// Update object in spatial index
    async fn update_object_spatial_index(&self, object_id: Uuid, object: &WorldObject) -> Result<()> {
        // For now, we'll rebuild the spatial index periodically
        // In production, you'd want a more efficient spatial index that supports updates
        Ok(())
    }
    
    /// Find region for a given position
    async fn find_region_for_position(&self, position: &Point3<f64>) -> Result<Uuid> {
        let spatial_index = self.spatial_index.read().await;
        let point = [position.x, position.y, position.z];
        
        // Create a tiny envelope around the point to query
        let point_envelope = AABB::from_corners(
            [position.x - 0.001, position.y - 0.001, position.z - 0.001],
            [position.x + 0.001, position.y + 0.001, position.z + 0.001],
        );
        
        // Find regions that contain this point
        let containing_regions: Vec<_> = spatial_index.locate_in_envelope_intersecting(&point_envelope).collect();
        
        if let Some(region) = containing_regions.first() {
            Ok(region.id)
        } else {
            // Create a new region if none exists for this position
            drop(spatial_index);
            let region_id = self.create_region_at_position(*position).await?;
            Ok(region_id)
        }
    }
    
    /// Create a region at a specific position
    async fn create_region_at_position(&self, position: Point3<f64>) -> Result<Uuid> {
        let region_id = Uuid::new_v4();
        let size = self.config.default_region_size;
        
        let region = WorldRegion {
            id: region_id,
            name: format!("Auto Region {}", region_id),
            center: position,
            size,
            bounds: AABB::from_corners(
                [position.x - size/2.0, position.y - size/2.0, position.z - size/2.0],
                [position.x + size/2.0, position.y + size/2.0, position.z + size/2.0],
            ),
            object_count: 0,
            max_objects: self.config.max_objects_per_region,
            region_type: RegionType::Normal,
            properties: RegionProperties {
                physics_enabled: self.config.physics_enabled,
                simulation_rate: self.config.simulation_tick_rate as f64,
                max_players: 1000,
                level_range: None,
                environment: "default".to_string(),
                weather: None,
                custom_data: serde_json::Value::Null,
            },
            created_at: chrono::Utc::now(),
            last_updated: chrono::Utc::now(),
            active: true,
            server_assignment: None,
        };
        
        self.add_region(region).await?;
        info!("🏞️  Auto-created region at position {:?}", position);
        
        Ok(region_id)
    }
    
    /// Query objects in a specific area
    #[instrument(skip(self))]
    pub async fn query_objects(&self, query: ObjectQuery) -> Result<Vec<WorldObject>> {
        let spatial_index = self.object_spatial_index.read().await;
        
        // Create search area
        let bounds = AABB::from_corners(
            [query.center.x - query.radius, query.center.y - query.radius, query.center.z - query.radius],
            [query.center.x + query.radius, query.center.y + query.radius, query.center.z + query.radius],
        );
        
        let mut results = Vec::new();
        
        // Find objects in the spatial index
        for spatial_obj in spatial_index.locate_in_envelope(&bounds) {
            if let Some(object) = self.objects.get(&spatial_obj.id) {
                // Apply filters
                if let Some(ref types) = query.object_types {
                    if !types.iter().any(|t| std::mem::discriminant(t) == std::mem::discriminant(&object.object_type)) {
                        continue;
                    }
                }
                
                if let Some(ref tags) = query.tags {
                    if !tags.iter().any(|tag| object.properties.tags.contains(tag)) {
                        continue;
                    }
                }
                
                // Check actual distance
                let distance = (object.position - query.center).norm();
                if distance <= query.radius {
                    results.push(object.clone());
                    
                    if let Some(limit) = query.limit {
                        if results.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    /// Calculate object bounds for spatial indexing
    fn calculate_object_bounds(&self, object: &WorldObject) -> AABB<[f64; 3]> {
        let radius = object.scale.norm() * 0.5; // Simplified bounding calculation
        
        AABB::from_corners(
            [object.position.x - radius, object.position.y - radius, object.position.z - radius],
            [object.position.x + radius, object.position.y + radius, object.position.z + radius],
        )
    }
    
    /// List all regions
    pub async fn list_regions(&self) -> Result<Vec<RegionInfo>> {
        let mut regions = Vec::new();
        
        for region in self.regions.iter() {
            regions.push(RegionInfo {
                name: region.name.clone(),
                size: region.size,
                object_count: region.object_count,
                active: region.active,
                region_type: region.region_type.clone(),
            });
        }
        
        Ok(regions)
    }
    
    /// Get world statistics
    pub async fn get_stats(&self) -> Result<WorldStats> {
        let total_regions = self.regions.len();
        let total_objects = self.objects.len();
        
        // Count active players
        let active_players = self.objects.iter()
            .filter(|obj| matches!(obj.object_type, ObjectType::Player))
            .count();
        
        // Calculate memory usage (simplified)
        let memory_usage_mb = (total_regions * 1024 + total_objects * 512) as u64;
        
        // Get region assignments
        let mut region_assignments = std::collections::HashMap::new();
        for region in self.regions.iter() {
            if let Some(server_id) = region.server_assignment {
                region_assignments.insert(region.id, server_id);
            }
        }
        
        Ok(WorldStats {
            total_regions,
            total_objects,
            active_players,
            memory_usage_mb,
            simulation_fps: self.config.simulation_tick_rate as f64,
            region_assignments,
        })
    }
    
    /// Get region count
    pub async fn get_region_count(&self) -> usize {
        self.regions.len()
    }
    
    /// Run the world manager
    pub async fn run(&self) -> Result<()> {
        info!("🌍 Starting world simulation");
        
        if self.config.physics_enabled {
            self.start_physics_simulation().await?;
        }
        
        self.start_region_maintenance().await?;
        
        // Keep running until shutdown
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
    
    /// Start physics simulation
    async fn start_physics_simulation(&self) -> Result<()> {
        let objects = self.objects.clone();
        let tick_rate = self.config.physics_tick_rate;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs_f64(1.0 / tick_rate as f64)
            );
            
            loop {
                interval.tick().await;
                
                // Simple physics simulation
                for mut object in objects.iter_mut() {
                    // Apply velocity to position
                    if object.velocity.norm() > 0.0 {
                        // Store velocity calculation in a temporary variable
                        let position_delta = object.velocity * (1.0 / tick_rate as f64);
                        object.position += position_delta;
                        object.last_updated = chrono::Utc::now();
                    }
                }
            }
        });
        
        Ok(())
    }
    
    /// Start region maintenance
    async fn start_region_maintenance(&self) -> Result<()> {
        let regions = self.regions.clone();
        let objects = self.objects.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // Clean up empty regions
                let mut regions_to_remove = Vec::new();
                for region in regions.iter() {
                    if region.object_count == 0 && region.name.starts_with("Auto Region") {
                        regions_to_remove.push(region.id);
                    }
                }
                
                for region_id in regions_to_remove {
                    regions.remove(&region_id);
                    debug!("🧹 Cleaned up empty auto region: {}", region_id);
                }
                
                // Clean up inactive objects
                let mut objects_to_remove = Vec::new();
                for object in objects.iter() {
                    if !object.active {
                        objects_to_remove.push(object.id);
                    }
                }
                
                for object_id in objects_to_remove {
                    objects.remove(&object_id);
                    debug!("🧹 Cleaned up inactive object: {}", object_id);
                }
            }
        });
        
        Ok(())
    }
    
    /// Health check
    pub async fn health_check(&self) -> Result<()> {
        // Check if critical systems are working
        if self.regions.is_empty() {
            return Err(anyhow::anyhow!("No regions available"));
        }
        
        // Check if spatial indices are functional
        {
            let _spatial_index = self.spatial_index.read().await;
            let _object_spatial_index = self.object_spatial_index.read().await;
        }
        
        Ok(())
    }
    
    /// Shutdown world manager
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down world manager");
        
        // Stop simulation
        if let Some(handle) = &self.simulation_handle {
            handle.abort();
        }
        
        // Save all persistent objects
        for object in self.objects.iter() {
            if object.properties.persistent {
                todo!("Save object {} to storage", object.id);
            }
        }
        
        // Save all regions
        for region in self.regions.iter() {
            todo!("Save region {} to storage", region.id);
        }
        
        info!("✅ World manager shutdown complete");
        Ok(())
    }
}