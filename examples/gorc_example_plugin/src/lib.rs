//! # GORC Example Plugin with Object Registration
//!
//! This plugin demonstrates how to use Game Object Replication Channels (GORC)
//! for efficient multiplayer game state distribution, including the new object
//! registration system.

use horizon_event_system::{
    create_simple_plugin, async_trait, defObject,
    SimplePlugin, PluginError, EventSystem, ReplicationLayer, ReplicationLayers, ReplicationPriority, CompressionType, 
    Vec3, PlayerId, Replication, GorcObjectRegistry, GorcEvent, MineralType, ServerContext,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug};

/// A game object that uses GORC for replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub position: Vec3,
    pub health: f32,
    pub max_health: f32,
    pub weapon: String,
    pub animation_state: String,
    pub team_id: Option<String>,
    pub in_combat: bool,
}

impl Player {
    pub fn new(id: PlayerId, position: Vec3) -> Self {
        Self {
            id,
            position,
            health: 100.0,
            max_health: 100.0,
            weapon: "basic_weapon".to_string(),
            animation_state: "idle".to_string(),
            team_id: None,
            in_combat: false,
        }
    }

    fn calculate_distance(&self, observer_pos: Vec3) -> f32 {
        self.position.distance(observer_pos)
    }

    fn serialize_critical_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "id": self.id,
            "position": self.position,
            "health": self.health,
            "in_combat": self.in_combat
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_detailed_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "id": self.id,
            "weapon": self.weapon,
            "animation_state": self.animation_state,
            "max_health": self.max_health
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_cosmetic_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "id": self.id,
            "animation_state": self.animation_state,
            "visual_effects": self.get_visual_effects()
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_metadata(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let data = serde_json::json!({
            "id": self.id,
            "team_id": self.team_id,
            "player_name": format!("Player_{}", self.id.to_string().chars().take(8).collect::<String>()),
            "level": 42,
            "achievements": ["first_kill", "team_player"]
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn get_visual_effects(&self) -> Vec<String> {
        let mut effects = Vec::new();
        if self.in_combat {
            effects.push("combat_glow".to_string());
        }
        if self.health < 25.0 {
            effects.push("low_health_effect".to_string());
        }
        effects
    }
}

impl Replication for Player {
    fn init_layers() -> ReplicationLayers {
        ReplicationLayers::new()
            // Critical Channel: Essential game state at 60Hz
            .add_layer(ReplicationLayer::new(
                0,
                100.0, // 100 unit radius for critical updates
                60.0,  // 60Hz for real-time combat
                vec![
                    "position".to_string(),
                    "health".to_string(),
                    "in_combat".to_string(),
                ],
                CompressionType::None, // No compression for speed
            ))
            // Detailed Channel: Important gameplay info at 30Hz
            .add_layer(ReplicationLayer::new(
                1,
                250.0, // Larger radius for weapon/animation awareness
                30.0,  // 30Hz for smooth animations
                vec![
                    "weapon".to_string(),
                    "animation_state".to_string(),
                    "max_health".to_string(),
                ],
                CompressionType::Lz4, // Light compression
            ))
            // Cosmetic Channel: Visual effects at 15Hz
            .add_layer(ReplicationLayer::new(
                2,
                400.0, // Wide radius for environmental awareness
                15.0,  // 15Hz sufficient for visual effects
                vec![
                    "animation_state".to_string(),
                    "visual_effects".to_string(),
                ],
                CompressionType::Zlib, // More compression acceptable
            ))
            // Metadata Channel: Player info at 5Hz
            .add_layer(ReplicationLayer::new(
                3,
                1000.0, // Very wide radius for social features
                5.0,    // Low frequency for rarely changing data
                vec![
                    "team_id".to_string(),
                    "player_name".to_string(),
                    "level".to_string(),
                    "achievements".to_string(),
                ],
                CompressionType::Zlib, // Maximize compression
            ))
    }

    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.calculate_distance(observer_pos);
        
        // Priority based on distance and combat state
        if self.in_combat && distance < 150.0 {
            ReplicationPriority::Critical
        } else if distance < 100.0 {
            ReplicationPriority::High
        } else if distance < 300.0 {
            ReplicationPriority::Normal
        } else {
            ReplicationPriority::Low
        }
    }

    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        match layer.channel {
            0 => self.serialize_critical_data(),
            1 => self.serialize_detailed_data(),
            2 => self.serialize_cosmetic_data(),
            3 => self.serialize_metadata(),
            _ => Ok(vec![]),
        }
    }
}

// Register Player object with GORC
defObject!(Player);

/// An asteroid game object that demonstrates the new object registration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asteroid {
    pub radius: i32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub health: f32,
    pub mineral_type: MineralType,
}

impl Asteroid {
    pub fn new(position: Vec3, radius: i32) -> Self {
        Self {
            radius,
            position,
            velocity: Vec3::new(0.0, 0.0, 0.0),
            health: 100.0,
            mineral_type: MineralType::Iron,
        }
    }
}

impl Replication for Asteroid {
    fn init_layers() -> ReplicationLayers {
        ReplicationLayers::new()
            .add_layer(ReplicationLayer {
                channel: 0,
                radius: 50.0,
                frequency: 30.0,
                properties: vec!["position".to_string(), "velocity".to_string(), "health".to_string()],
                compression: CompressionType::Delta,
                priority: ReplicationPriority::Critical,
            })
            .add_layer(ReplicationLayer {
                channel: 1,
                radius: 200.0,
                frequency: 10.0,
                properties: vec!["position".to_string(), "mineral_type".to_string()],
                compression: CompressionType::Quantized,
                priority: ReplicationPriority::High,
            })
            .add_layer(ReplicationLayer {
                channel: 2,
                radius: 1000.0,
                frequency: 2.0,
                properties: vec!["position".to_string()],
                compression: CompressionType::High,
                priority: ReplicationPriority::Normal,
            })
    }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        match distance {
            d if d < 25.0 => ReplicationPriority::Critical,
            d if d < 100.0 => ReplicationPriority::High,
            d if d < 500.0 => ReplicationPriority::Normal,
            _ => ReplicationPriority::Low,
        }
    }

    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        match layer.channel {
            0 => {
                let data = serde_json::json!({
                    "position": self.position,
                    "velocity": self.velocity,
                    "health": self.health
                });
                Ok(serde_json::to_vec(&data)?)
            },
            1 => {
                let data = serde_json::json!({
                    "position": self.position,
                    "mineral_type": self.mineral_type
                });
                Ok(serde_json::to_vec(&data)?)
            },
            2 => {
                let data = serde_json::json!({
                    "position": self.position
                });
                Ok(serde_json::to_vec(&data)?)
            },
            _ => Ok(vec![]),
        }
    }
}

// Register Asteroid object with GORC
defObject!(Asteroid);

/// Plugin for demonstrating GORC functionality with object registration
pub struct GorcExamplePlugin {
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    asteroids: Arc<RwLock<HashMap<String, Asteroid>>>,
    object_registry: Arc<GorcObjectRegistry>,
}

impl GorcExamplePlugin {
    pub fn new() -> Self {
        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            asteroids: Arc::new(RwLock::new(HashMap::new())),
            object_registry: Arc::new(GorcObjectRegistry::new()),
        }
    }

    async fn setup_object_registry(&self) -> Result<(), String> {
        // Register Player and Asteroid object types
        Player::register_with_gorc(self.object_registry.clone()).await
            .map_err(|e| e.to_string())?;
        Asteroid::register_with_gorc(self.object_registry.clone()).await
            .map_err(|e| e.to_string())?;
        
        let objects = self.object_registry.list_objects().await;
        info!("📦 Registered GORC objects: {:?}", objects);
        
        Ok(())
    }

    async fn setup_gorc_handlers(&self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        // Register GORC event handlers for Player objects
        events.on_gorc("Player", 0, "position_update", |event: GorcEvent| {
            info!("🎯 Player critical position update: {}", event.object_id);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_gorc("Player", 1, "weapon_change", |event: GorcEvent| {
            info!("🔫 Player weapon change: {}", event.object_id);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        // Register GORC event handlers for Asteroid objects
        events.on_gorc("Asteroid", 0, "position_update", |event: GorcEvent| {
            debug!("🌌 Asteroid position update: {}", event.object_id);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_gorc("Asteroid", 1, "mineral_scan", |event: GorcEvent| {
            info!("⛏️ Asteroid mineral scan: {}", event.object_id);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        Ok(())
    }

    async fn demonstrate_object_replication(&self, events: Arc<EventSystem>) -> Result<(), String> {
        // Create a player and demonstrate replication
        let player = Player::new(PlayerId::new(), Vec3::new(100.0, 50.0, 200.0));
        let player_id = player.id;
        
        {
            let mut players = self.players.write().await;
            players.insert(player_id, player.clone());
        }

        // Emit GORC events for the player
        let critical_data = player.serialize_for_layer(&ReplicationLayer::new(
            0, 100.0, 60.0, vec!["position".to_string()], CompressionType::None
        )).map_err(|e| format!("Serialization error: {}", e))?;

        events.emit_gorc("Player", 0, "position_update", &GorcEvent {
            object_id: player_id.to_string(),
            object_type: "Player".to_string(),
            channel: 0,
            data: critical_data,
            priority: "Critical".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        }).await.map_err(|e| e.to_string())?;

        // Create an asteroid and demonstrate replication
        let asteroid = Asteroid::new(Vec3::new(500.0, 100.0, 300.0), 25);
        let asteroid_id = "asteroid_001".to_string();
        
        {
            let mut asteroids = self.asteroids.write().await;
            asteroids.insert(asteroid_id.clone(), asteroid.clone());
        }

        // Emit GORC events for the asteroid
        let mineral_data = asteroid.serialize_for_layer(&ReplicationLayer {
            channel: 1,
            radius: 200.0,
            frequency: 10.0,
            properties: vec!["mineral_type".to_string()],
            compression: CompressionType::Quantized,
            priority: ReplicationPriority::High,
        }).map_err(|e| format!("Serialization error: {}", e))?;

        events.emit_gorc("Asteroid", 1, "mineral_scan", &GorcEvent {
            object_id: asteroid_id,
            object_type: "Asteroid".to_string(),
            channel: 1,
            data: mineral_data,
            priority: "High".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs(),
        }).await.map_err(|e| e.to_string())?;

        info!("✨ Demonstrated object replication for Player and Asteroid");
        Ok(())
    }
}

#[async_trait]
impl SimplePlugin for GorcExamplePlugin {
    fn name(&self) -> &str {
        "GORC Example Plugin with Object Registration"
    }

    fn version(&self) -> &str {
        "2.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        info!("🚀 Initializing GORC Example Plugin with Object Registration");
        
        // Setup object registry
        self.setup_object_registry().await
            .map_err(|e| PluginError::ExecutionError(e))?;
        
        // Setup GORC handlers
        self.setup_gorc_handlers(events.clone()).await?;
        
        // Register traditional event handlers
        events.on_client("player", "join", |event: serde_json::Value| {
            info!("📥 Player join event: {:?}", event);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_client("player", "move", |event: serde_json::Value| {
            debug!("🏃 Player move event: {:?}", event);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_client("asteroid", "spawn", |event: serde_json::Value| {
            info!("🌌 Asteroid spawn event: {:?}", event);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_core("player_connected", |event: serde_json::Value| {
            info!("🔗 Core: Player connected: {:?}", event);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;

        events.on_core("player_disconnected", |event: serde_json::Value| {
            info!("👋 Core: Player disconnected: {:?}", event);
            Ok(())
        }).await.map_err(|e| PluginError::ExecutionError(e.to_string()))?;
        
        // Demonstrate object replication
        self.demonstrate_object_replication(events).await
            .map_err(|e| PluginError::ExecutionError(e))?;
        
        info!("✅ GORC Example Plugin with Object Registration initialized");
        Ok(())
    }
}

// Create the plugin (required for dynamic loading)
create_simple_plugin!(GorcExamplePlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_replication_layers() {
        let layers = Player::init_layers();
        assert_eq!(layers.len(), 4);
        
        let layer_vec = layers.into_layers();
        assert_eq!(layer_vec[0].channel, 0); // Critical
        assert_eq!(layer_vec[1].channel, 1); // Detailed
        assert_eq!(layer_vec[2].channel, 2); // Cosmetic
        assert_eq!(layer_vec[3].channel, 3); // Metadata
    }

    #[test]
    fn test_asteroid_replication_layers() {
        let layers = Asteroid::init_layers();
        assert_eq!(layers.len(), 3);
        
        let layer_vec = layers.into_layers();
        assert_eq!(layer_vec[0].channel, 0); // Critical
        assert_eq!(layer_vec[1].channel, 1); // Detailed  
        assert_eq!(layer_vec[2].channel, 2); // Cosmetic
    }

    #[test]
    fn test_asteroid_priority_calculation() {
        let asteroid = Asteroid::new(Vec3::new(0.0, 0.0, 0.0), 10);
        
        let close_pos = Vec3::new(20.0, 0.0, 0.0);
        assert_eq!(asteroid.get_priority(close_pos), ReplicationPriority::Critical);
        
        let far_pos = Vec3::new(600.0, 0.0, 0.0);
        assert_eq!(asteroid.get_priority(far_pos), ReplicationPriority::Low);
    }

    #[tokio::test]
    async fn test_object_registration() {
        let registry = Arc::new(GorcObjectRegistry::new());

        Player::register_with_gorc(registry.clone()).await.unwrap();
        Asteroid::register_with_gorc(registry.clone()).await.unwrap();
        
        let objects = registry.list_objects().await;
        assert_eq!(objects.len(), 2);
        assert!(objects.contains(&"Player".to_string()));
        assert!(objects.contains(&"Asteroid".to_string()));
    }

    #[tokio::test]
    async fn test_plugin_initialization() {
        let plugin = GorcExamplePlugin::new();
        assert_eq!(plugin.name(), "GORC Example Plugin with Object Registration");
        assert_eq!(plugin.version(), "2.0.0");
    }
}