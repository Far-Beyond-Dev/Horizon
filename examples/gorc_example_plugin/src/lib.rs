//! # GORC Example Plugin
//!
//! This plugin demonstrates how to use Game Object Replication Channels (GORC)
//! for efficient multiplayer game state distribution.

use horizon_event_system::{
    create_simple_plugin, register_handlers, async_trait, on_event,
    SimplePlugin, PluginError, EventSystem, ServerContext, EventError,
    // GORC imports
    GorcManager, SubscriptionManager, MulticastManager, SpatialPartition,
    ReplicationLayer, ReplicationPriority, CompressionType, Position, PlayerId,
    Replication,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, debug, warn};

/// A game object that uses GORC for replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub position: Position,
    pub health: f32,
    pub max_health: f32,
    pub weapon: String,
    pub animation_state: String,
    pub team_id: Option<String>,
    pub in_combat: bool,
}

impl Player {
    pub fn new(id: PlayerId, position: Position) -> Self {
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

    fn calculate_distance(&self, observer_pos: Position) -> f32 {
        let dx = self.position.x - observer_pos.x;
        let dy = self.position.y - observer_pos.y;
        let dz = self.position.z - observer_pos.z;
        ((dx * dx + dy * dy + dz * dz) as f32).sqrt()
    }

    fn serialize_critical_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let data = serde_json::json!({
            "id": self.id,
            "position": self.position,
            "health": self.health,
            "in_combat": self.in_combat
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_detailed_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let data = serde_json::json!({
            "id": self.id,
            "weapon": self.weapon,
            "animation_state": self.animation_state,
            "max_health": self.max_health
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_cosmetic_data(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let data = serde_json::json!({
            "id": self.id,
            "animation_state": self.animation_state,
            "visual_effects": self.get_visual_effects()
        });
        Ok(serde_json::to_vec(&data)?)
    }

    fn serialize_metadata(&self) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
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
    fn init_layers() -> Vec<ReplicationLayer> {
        vec![
            // Critical Channel: Essential game state at 60Hz
            ReplicationLayer::new(
                0,
                100.0, // 100 unit radius for critical updates
                60.0,  // 60Hz for real-time combat
                vec![
                    "position".to_string(),
                    "health".to_string(),
                    "in_combat".to_string(),
                ],
                CompressionType::None, // No compression for speed
            ),
            // Detailed Channel: Important gameplay info at 30Hz
            ReplicationLayer::new(
                1,
                250.0, // Larger radius for weapon/animation awareness
                30.0,  // 30Hz for smooth animations
                vec![
                    "weapon".to_string(),
                    "animation_state".to_string(),
                    "max_health".to_string(),
                ],
                CompressionType::Lz4, // Light compression
            ),
            // Cosmetic Channel: Visual effects at 15Hz
            ReplicationLayer::new(
                2,
                400.0, // Wide radius for environmental awareness
                15.0,  // 15Hz sufficient for visual effects
                vec![
                    "animation_state".to_string(),
                    "visual_effects".to_string(),
                ],
                CompressionType::Zlib, // More compression acceptable
            ),
            // Metadata Channel: Player info at 5Hz
            ReplicationLayer::new(
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
            ),
        ]
    }

    fn get_priority(&self, observer_pos: Position) -> ReplicationPriority {
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
            0 => self.serialize_critical_data().map_err(|e| e.into()),
            1 => self.serialize_detailed_data().map_err(|e| e.into()),
            2 => self.serialize_cosmetic_data().map_err(|e| e.into()),
            3 => self.serialize_metadata().map_err(|e| e.into()),
            _ => Ok(vec![]),
        }
    }
}

/// Plugin for demonstrating GORC functionality
pub struct GorcExamplePlugin {
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    team_groups: Arc<RwLock<HashMap<String, Vec<PlayerId>>>>,
}

impl GorcExamplePlugin {
    pub fn new() -> Self {
        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            team_groups: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn setup_gorc_layers(&self, gorc_manager: Arc<GorcManager>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let layers = Player::init_layers();
        
        for (i, layer) in layers.into_iter().enumerate() {
            let layer_name = format!("player_layer_{}", i);
            gorc_manager.add_layer(layer_name, layer).await;
            info!("📡 Registered GORC layer {}", i);
        }
        
        Ok(())
    }

    async fn handle_player_join(
        &self,
        player_id: PlayerId,
        position: Position,
        subscription_manager: Arc<SubscriptionManager>,
        spatial_partition: Arc<SpatialPartition>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let player = Player::new(player_id, position);
        
        // Add to our local player registry
        {
            let mut players = self.players.write().await;
            players.insert(player_id, player);
        }
        
        // Register with GORC subscription system
        subscription_manager.add_player(player_id, position).await;
        
        // Add to spatial partition
        spatial_partition.update_player_position(
            player_id,
            position,
            "main_world".to_string(),
        ).await;
        
        info!("🎮 Player {} joined at position {:?}", player_id, position);
        Ok(())
    }

    async fn handle_player_move(
        &self,
        player_id: PlayerId,
        new_position: Position,
        subscription_manager: Arc<SubscriptionManager>,
        spatial_partition: Arc<SpatialPartition>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update player position
        {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(&player_id) {
                player.position = new_position;
            }
        }
        
        // Update GORC systems
        let should_recalc = subscription_manager.update_player_position(player_id, new_position).await;
        
        spatial_partition.update_player_position(
            player_id,
            new_position,
            "main_world".to_string(),
        ).await;
        
        if should_recalc {
            debug!("🔄 Recalculated subscriptions for player {}", player_id);
        }
        
        Ok(())
    }

    async fn handle_team_formation(
        &self,
        team_id: String,
        player_ids: Vec<PlayerId>,
        subscription_manager: Arc<SubscriptionManager>,
        multicast_manager: Arc<MulticastManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create multicast group for team
        let channels: HashSet<u8> = vec![0, 1, 2, 3].into_iter().collect();
        let group_id = multicast_manager.create_group(
            format!("team_{}", team_id),
            channels,
            ReplicationPriority::High,
        ).await;
        
        // Add all team members to the group
        for &player_id in &player_ids {
            multicast_manager.add_player_to_group(player_id, group_id).await;
            
            // Set up relationship-based subscriptions
            let other_members: Vec<PlayerId> = player_ids.iter()
                .filter(|&&id| id != player_id)
                .copied()
                .collect();
            
            subscription_manager.add_relationship(
                player_id,
                "team".to_string(),
                other_members,
            ).await;
        }
        
        // Store team information
        {
            let mut teams = self.team_groups.write().await;
            teams.insert(team_id.clone(), player_ids.clone());
        }
        
        info!("👥 Team {} formed with {} members", team_id, player_ids.len());
        Ok(())
    }

    async fn handle_combat_start(
        &self,
        player_id: PlayerId,
        multicast_manager: Arc<MulticastManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Update player combat state
        {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(&player_id) {
                player.in_combat = true;
                player.animation_state = "combat".to_string();
            }
        }
        
        // Broadcast combat start to nearby players
        // In a real implementation, you'd use spatial queries to find nearby players
        // and create a temporary high-priority group for combat updates
        
        info!("⚔️ Player {} entered combat", player_id);
        Ok(())
    }
}

#[async_trait]
impl SimplePlugin for GorcExamplePlugin {
    fn name(&self) -> &str {
        "GORC Example Plugin"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn register_handlers(&mut self, events: Arc<EventSystem>) -> Result<(), PluginError> {
        info!("🚀 Initializing GORC Example Plugin");
        
        // Note: In a real implementation, you'd get access to GORC managers
        // through the ServerContext or a custom service container
        
        register_handlers!(events;
            client {
                "player", "join" => |event: serde_json::Value| {
                    info!("📥 Player join event: {:?}", event);
                    // Handle player joining
                    Ok(())
                },
                "player", "move" => |event: serde_json::Value| {
                    debug!("🏃 Player move event: {:?}", event);
                    // Handle player movement
                    Ok(())
                },
                "team", "form" => |event: serde_json::Value| {
                    info!("👥 Team formation event: {:?}", event);
                    // Handle team formation
                    Ok(())
                },
                "combat", "start" => |event: serde_json::Value| {
                    info!("⚔️ Combat start event: {:?}", event);
                    // Handle combat start
                    Ok(())
                }
            }
            core {
                "player_connected" => |event: serde_json::Value| {
                    info!("🔗 Core: Player connected: {:?}", event);
                    Ok(())
                },
                "player_disconnected" => |event: serde_json::Value| {
                    info!("👋 Core: Player disconnected: {:?}", event);
                    Ok(())
                }
            }
        );
        
        info!("✅ GORC Example Plugin handlers registered");
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
        
        // Verify channel assignments
        assert_eq!(layers[0].channel, 0); // Critical
        assert_eq!(layers[1].channel, 1); // Detailed
        assert_eq!(layers[2].channel, 2); // Cosmetic
        assert_eq!(layers[3].channel, 3); // Metadata
        
        // Verify frequency hierarchy
        assert!(layers[0].frequency >= layers[1].frequency);
        assert!(layers[1].frequency >= layers[2].frequency);
        assert!(layers[2].frequency >= layers[3].frequency);
    }

    #[test]
    fn test_player_priority_calculation() {
        let player = Player::new(
            PlayerId::new(),
            Position::new(0.0, 0.0, 0.0),
        );
        
        // Close observer should get high priority
        let close_pos = Position::new(50.0, 0.0, 0.0);
        let priority = player.get_priority(close_pos);
        assert_eq!(priority, ReplicationPriority::High);
        
        // Far observer should get lower priority
        let far_pos = Position::new(500.0, 0.0, 0.0);
        let priority = player.get_priority(far_pos);
        assert_eq!(priority, ReplicationPriority::Low);
    }

    #[test]
    fn test_player_serialization() {
        let player = Player::new(
            PlayerId::new(),
            Position::new(100.0, 50.0, 200.0),
        );
        
        let layer = ReplicationLayer::new(
            0, 100.0, 60.0,
            vec!["position".to_string(), "health".to_string()],
            CompressionType::None,
        );
        
        let serialized = player.serialize_for_layer(&layer);
        assert!(serialized.is_ok());
        assert!(!serialized.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_plugin_initialization() {
        let plugin = GorcExamplePlugin::new();
        assert_eq!(plugin.name(), "GORC Example Plugin");
        assert_eq!(plugin.version(), "1.0.0");
        
        let players = plugin.players.read().await;
        assert_eq!(players.len(), 0);
    }
}