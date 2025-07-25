//! Example implementations for common game objects.
//!
//! This module provides reference implementations of the `GorcObject` trait
//! for typical game entities, demonstrating best practices for replication
//! layer configuration and property serialization.

use super::{CompressionType, GorcObject, MineralType, ReplicationLayer, ReplicationPriority};
use crate::types::Vec3;
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Example asteroid implementation showcasing mining game mechanics.
/// 
/// This asteroid demonstrates how to implement multi-layer replication
/// for objects with both physical properties (position, collision) and
/// strategic information (mineral type, value).
/// 
/// # Replication Strategy
/// 
/// * **Critical Layer**: Position, velocity, and health for collision detection
/// * **Detailed Layer**: Rotation speed for visual effects
/// * **Metadata Layer**: Mineral type and radius for strategic planning
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExampleAsteroid {
    /// 3D position in world space
    pub position: Vec3,
    /// Current velocity vector
    pub velocity: Vec3,
    /// Current health/integrity (0.0 = destroyed)
    pub health: f32,
    /// Physical radius for collision detection
    pub radius: f32,
    /// Type of mineral contained in this asteroid
    pub mineral_type: MineralType,
    /// Rotation speed for visual effects
    pub rotation_speed: f32,
}

impl ExampleAsteroid {
    /// Creates a new asteroid at the specified position.
    /// 
    /// # Arguments
    /// 
    /// * `position` - Initial 3D position
    /// * `mineral_type` - Type of mineral this asteroid contains
    /// 
    /// # Returns
    /// 
    /// A new asteroid instance with default properties.
    pub fn new(position: Vec3, mineral_type: MineralType) -> Self {
        Self {
            position,
            velocity: Vec3::new(0.0, 0.0, 0.0),
            health: 100.0,
            radius: 10.0,
            mineral_type,
            rotation_speed: 1.0,
        }
    }
}

impl GorcObject for ExampleAsteroid {
    fn type_name(&self) -> &'static str { "ExampleAsteroid" }
    
    fn position(&self) -> Vec3 { self.position }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        if distance < 100.0 { ReplicationPriority::Critical }
        else if distance < 300.0 { ReplicationPriority::High }
        else if distance < 1000.0 { ReplicationPriority::Normal }
        else { ReplicationPriority::Low }
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut data = serde_json::Map::new();
        
        for property in &layer.properties {
            match property.as_str() {
                "position" => {
                    data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                }
                "velocity" => {
                    data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                }
                "health" => {
                    data.insert("health".to_string(), serde_json::to_value(self.health)?);
                }
                "radius" => {
                    data.insert("radius".to_string(), serde_json::to_value(self.radius)?);
                }
                "mineral_type" => {
                    data.insert("mineral_type".to_string(), serde_json::to_value(&self.mineral_type)?);
                }
                "rotation_speed" => {
                    data.insert("rotation_speed".to_string(), serde_json::to_value(self.rotation_speed)?);
                }
                _ => {} // Ignore unknown properties
            }
        }
        
        Ok(serde_json::to_vec(&data)?)
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            // Critical: Position and collision data
            ReplicationLayer::new(
                0, 100.0, 30.0,
                vec!["position".to_string(), "velocity".to_string(), "health".to_string()],
                CompressionType::Delta
            ),
            // Detailed: Visual state
            ReplicationLayer::new(
                1, 300.0, 15.0,
                vec!["rotation_speed".to_string()],
                CompressionType::Lz4
            ),
            // Metadata: Strategic information
            ReplicationLayer::new(
                3, 2000.0, 2.0,
                vec!["mineral_type".to_string(), "radius".to_string()],
                CompressionType::High
            ),
        ]
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.position = new_position;
    }
    
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}

/// Example player implementation for multiplayer games.
/// 
/// This player object demonstrates how to handle frequently-changing data
/// (position, movement) alongside more static information (name, level).
/// 
/// # Replication Strategy
/// 
/// * **Critical Layer**: Essential player state at high frequency
/// * **Detailed Layer**: Equipment and visual state
/// * **Metadata Layer**: Player information and progression data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExamplePlayer {
    /// Current 3D position
    pub position: Vec3,
    /// Current velocity vector
    pub velocity: Vec3,
    /// Current health points
    pub health: f32,
    /// Player display name
    pub name: String,
    /// Player level/experience
    pub level: u32,
    /// List of equipped items
    pub equipment: Vec<String>,
}

impl ExamplePlayer {
    /// Creates a new player instance.
    /// 
    /// # Arguments
    /// 
    /// * `name` - Player's display name
    /// * `position` - Initial spawn position
    /// 
    /// # Returns
    /// 
    /// A new player instance with default stats.
    pub fn new(name: String, position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::new(0.0, 0.0, 0.0),
            health: 100.0,
            name,
            level: 1,
            equipment: Vec::new(),
        }
    }
}

impl GorcObject for ExamplePlayer {
    fn type_name(&self) -> &'static str { "ExamplePlayer" }
    
    fn position(&self) -> Vec3 { self.position }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        if distance < 50.0 { ReplicationPriority::Critical }
        else if distance < 200.0 { ReplicationPriority::High }
        else if distance < 500.0 { ReplicationPriority::Normal }
        else { ReplicationPriority::Low }
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut data = serde_json::Map::new();
        
        for property in &layer.properties {
            match property.as_str() {
                "position" => {
                    data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                }
                "velocity" => {
                    data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                }
                "health" => {
                    data.insert("health".to_string(), serde_json::to_value(self.health)?);
                }
                "name" => {
                    data.insert("name".to_string(), serde_json::to_value(&self.name)?);
                }
                "level" => {
                    data.insert("level".to_string(), serde_json::to_value(self.level)?);
                }
                "equipment" => {
                    data.insert("equipment".to_string(), serde_json::to_value(&self.equipment)?);
                }
                _ => {} // Ignore unknown properties
            }
        }
        
        Ok(serde_json::to_vec(&data)?)
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            // Critical: Essential player state
            ReplicationLayer::new(
                0, 50.0, 60.0,
                vec!["position".to_string(), "velocity".to_string(), "health".to_string()],
                CompressionType::Delta
            ),
            // Detailed: Equipment and visual state
            ReplicationLayer::new(
                1, 200.0, 20.0,
                vec!["equipment".to_string()],
                CompressionType::Lz4
            ),
            // Metadata: Player information
            ReplicationLayer::new(
                3, 1000.0, 5.0,
                vec!["name".to_string(), "level".to_string()],
                CompressionType::High
            ),
        ]
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.position = new_position;
    }
    
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}

/// Example projectile implementation for fast-moving objects.
/// 
/// Demonstrates how to handle objects that require high-frequency updates
/// for critical data while minimizing bandwidth usage for less important
/// properties.
/// 
/// # Replication Strategy
/// 
/// * **Critical Layer**: Position and velocity at maximum frequency
/// * **Metadata Layer**: Damage and owner information at low frequency
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExampleProjectile {
    /// Current 3D position
    pub position: Vec3,
    /// Current velocity vector
    pub velocity: Vec3,
    /// Damage this projectile will inflict
    pub damage: f32,
    /// ID of the player who fired this projectile
    pub owner_id: String,
    /// Projectile type identifier
    pub projectile_type: String,
}

impl ExampleProjectile {
    /// Creates a new projectile.
    /// 
    /// # Arguments
    /// 
    /// * `position` - Initial firing position
    /// * `velocity` - Initial velocity vector
    /// * `damage` - Damage amount
    /// * `owner_id` - ID of the firing player
    /// * `projectile_type` - Type identifier
    pub fn new(
        position: Vec3,
        velocity: Vec3,
        damage: f32,
        owner_id: String,
        projectile_type: String,
    ) -> Self {
        Self {
            position,
            velocity,
            damage,
            owner_id,
            projectile_type,
        }
    }
}

impl GorcObject for ExampleProjectile {
    fn type_name(&self) -> &'static str { "ExampleProjectile" }
    
    fn position(&self) -> Vec3 { self.position }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        // Projectiles are always high priority when visible due to their speed
        if distance < 100.0 { ReplicationPriority::Critical }
        else if distance < 500.0 { ReplicationPriority::High }
        else { ReplicationPriority::Normal }
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut data = serde_json::Map::new();
        
        for property in &layer.properties {
            match property.as_str() {
                "position" => {
                    data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                }
                "velocity" => {
                    data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                }
                "damage" => {
                    data.insert("damage".to_string(), serde_json::to_value(self.damage)?);
                }
                "owner_id" => {
                    data.insert("owner_id".to_string(), serde_json::to_value(&self.owner_id)?);
                }
                "projectile_type" => {
                    data.insert("projectile_type".to_string(), serde_json::to_value(&self.projectile_type)?);
                }
                _ => {}
            }
        }
        
        Ok(serde_json::to_vec(&data)?)
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            // Critical: Position and velocity at maximum frequency for smooth trajectory
            ReplicationLayer::new(
                0, 200.0, 60.0, // Larger range and higher frequency for projectiles
                vec!["position".to_string(), "velocity".to_string()],
                CompressionType::Delta
            ),
            // Metadata: Static properties that rarely change
            ReplicationLayer::new(
                3, 500.0, 1.0, // Very low frequency for static data
                vec!["damage".to_string(), "owner_id".to_string(), "projectile_type".to_string()],
                CompressionType::High
            ),
        ]
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.position = new_position;
    }
    
    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}