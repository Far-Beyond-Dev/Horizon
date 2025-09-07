//! # GORC Player Object Implementation
//!
//! This module contains the complete GORC player object implementation,
//! demonstrating proper replication layer configuration for multiplayer games.

use horizon_event_system::{
    GorcObject, ReplicationLayer, ReplicationPriority, CompressionType,
    Vec3, PlayerId,
};
use serde::{Deserialize, Serialize};
use std::any::Any;
use chrono::{DateTime, Utc};
use thiserror::Error;

/// Custom error types for player plugin
#[derive(Error, Debug)]
pub enum PlayerPluginError {
    #[error("Player with ID {0} not found")]
    PlayerNotFound(PlayerId),
    #[error("GORC system not initialized")]
    GorcSystemNotInitialized,
    #[error("GORC system error: {0}")]
    GorcError(Box<dyn std::error::Error + Send + Sync>),
    #[error("Invalid player data: {0}")]
    InvalidPlayerData(String),
    #[error("Movement validation failed: {0}")]
    MovementValidationFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

/// Player health and status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerHealth {
    /// Current health points (0.0 - 100.0)
    pub current: f32,
    /// Maximum health points
    pub maximum: f32,
    /// Health regeneration rate per second
    pub regen_rate: f32,
    /// Whether the player is currently regenerating health
    pub is_regenerating: bool,
}

impl Default for PlayerHealth {
    fn default() -> Self {
        Self {
            current: 100.0,
            maximum: 100.0,
            regen_rate: 1.0,
            is_regenerating: false,
        }
    }
}

/// Player combat state and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerCombat {
    /// Base attack damage
    pub attack_damage: f32,
    /// Attack speed (attacks per second)
    pub attack_speed: f32,
    /// Last attack timestamp
    pub last_attack: Option<DateTime<Utc>>,
    /// Whether player is currently in combat
    pub in_combat: bool,
    /// Combat timeout (when to exit combat state)
    pub combat_timeout: Option<DateTime<Utc>>,
}

impl Default for PlayerCombat {
    fn default() -> Self {
        Self {
            attack_damage: 25.0,
            attack_speed: 1.0,
            last_attack: None,
            in_combat: false,
            combat_timeout: None,
        }
    }
}

/// Player equipment and inventory state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerEquipment {
    /// Currently equipped weapon
    pub weapon: Option<String>,
    /// Currently equipped armor pieces
    pub armor: Vec<String>,
    /// Equipped accessories
    pub accessories: Vec<String>,
    /// Quick-access items
    pub hotbar: [Option<String>; 10],
}

impl Default for PlayerEquipment {
    fn default() -> Self {
        Self {
            weapon: None,
            armor: Vec::new(),
            accessories: Vec::new(),
            hotbar: Default::default(),
        }
    }
}

/// Complete GORC player object with multi-layer replication
/// 
/// This implementation demonstrates proper separation of data into replication layers:
/// - **Critical Layer**: Essential state needed for collision and immediate gameplay
/// - **Detailed Layer**: Visual and gameplay state for nearby players
/// - **Social Layer**: Chat, emotes, and social interactions
/// - **Metadata Layer**: Character information and progression data
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GorcPlayer {
    // === Critical Layer Data (Channel 0) ===
    /// Current 3D position in world space
    pub position: Vec3,
    /// Current velocity vector
    pub velocity: Vec3,
    /// Player health information
    pub health: PlayerHealth,
    /// Player's unique identifier
    pub player_id: PlayerId,

    // === Detailed Layer Data (Channel 1) ===
    /// Player combat state and stats
    pub combat: PlayerCombat,
    /// Current equipment and gear
    pub equipment: PlayerEquipment,
    /// Player rotation/facing direction
    pub rotation: f32,
    /// Current movement state flags
    pub movement_state: u32, // Bitfield: walking, running, crouching, jumping, etc.

    // === Social Layer Data (Channel 2) ===
    /// Player's display name
    pub display_name: String,
    /// Current emote or animation state
    pub emote_state: Option<String>,
    /// Chat bubble text (temporary)
    pub chat_bubble: Option<String>,
    /// Player status flags (AFK, DND, etc.)
    pub status_flags: u32,

    // === Metadata Layer Data (Channel 3) ===
    /// Player level
    pub level: u32,
    /// Experience points
    pub experience: u64,
    /// Player class or specialization
    pub player_class: String,
    /// Guild or clan affiliation
    pub guild_info: Option<String>,
    /// Achievement badges to display
    pub achievement_badges: Vec<String>,
    
    // === Internal State (Not Replicated) ===
    /// Last update timestamp
    pub last_update: DateTime<Utc>,
    /// Last position update for delta compression
    pub last_position: Vec3,
    /// Movement validation data
    pub movement_validator: MovementValidator,
}

/// Server-side movement validation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MovementValidator {
    /// Maximum allowed movement speed
    pub max_speed: f32,
    /// Last validated position
    pub last_validated_position: Vec3,
    /// Last validation timestamp
    pub last_validation: DateTime<Utc>,
    /// Number of consecutive validation failures
    pub validation_failures: u32,
}

impl Default for MovementValidator {
    fn default() -> Self {
        Self {
            max_speed: 10.0, // meters per second
            last_validated_position: Vec3::zero(),
            last_validation: Utc::now(),
            validation_failures: 0,
        }
    }
}

impl GorcPlayer {
    /// Creates a new player instance with default values
    pub fn new(player_id: PlayerId, display_name: String, spawn_position: Vec3) -> Self {
        let now = Utc::now();
        Self {
            // Critical layer
            position: spawn_position,
            velocity: Vec3::zero(),
            health: PlayerHealth::default(),
            player_id,

            // Detailed layer
            combat: PlayerCombat::default(),
            equipment: PlayerEquipment::default(),
            rotation: 0.0,
            movement_state: 0,

            // Social layer
            display_name,
            emote_state: None,
            chat_bubble: None,
            status_flags: 0,

            // Metadata layer
            level: 1,
            experience: 0,
            player_class: "Adventurer".to_string(),
            guild_info: None,
            achievement_badges: Vec::new(),

            // Internal state
            last_update: now,
            last_position: spawn_position,
            movement_validator: MovementValidator {
                last_validated_position: spawn_position,
                last_validation: now,
                ..Default::default()
            },
        }
    }

    /// Validates and applies a movement update
    pub fn validate_and_apply_movement(&mut self, new_position: Vec3, new_velocity: Vec3) -> Result<(), PlayerPluginError> {
        let now = Utc::now();
        let time_delta = (now - self.movement_validator.last_validation).num_milliseconds() as f32 / 1000.0;
        
        // Validate movement speed
        let movement_distance = self.movement_validator.last_validated_position.distance(new_position);
        let max_possible_distance = self.movement_validator.max_speed * time_delta;
        
        if movement_distance > max_possible_distance.into() {
            self.movement_validator.validation_failures += 1;
            
            if self.movement_validator.validation_failures > 3 {
                return Err(PlayerPluginError::MovementValidationFailed(
                    format!("Player {} exceeded movement speed limit: {} > {} (failures: {})", 
                        self.player_id, 
                        movement_distance, 
                        max_possible_distance,
                        self.movement_validator.validation_failures
                    )
                ));
            }
        } else {
            // Reset failure counter on valid movement
            self.movement_validator.validation_failures = 0;
        }

        // Apply the movement
        self.last_position = self.position;
        self.position = new_position;
        self.velocity = new_velocity;
        self.last_update = now;
        self.movement_validator.last_validated_position = new_position;
        self.movement_validator.last_validation = now;

        Ok(())
    }

    /// Handles player attack action
    pub fn perform_attack(&mut self, _target_position: Vec3) -> Result<f32, PlayerPluginError> {
        let now = Utc::now();
        
        // Check attack cooldown
        if let Some(last_attack) = self.combat.last_attack {
            let cooldown_seconds = 1.0 / self.combat.attack_speed;
            let time_since_attack = (now - last_attack).num_milliseconds() as f32 / 1000.0;
            
            if time_since_attack < cooldown_seconds {
                return Err(PlayerPluginError::InvalidPlayerData(
                    format!("Attack on cooldown: {:.2}s remaining", cooldown_seconds - time_since_attack)
                ));
            }
        }

        // Update combat state
        self.combat.last_attack = Some(now);
        self.combat.in_combat = true;
        self.combat.combat_timeout = Some(now + chrono::Duration::seconds(10));
        self.last_update = now;

        Ok(self.combat.attack_damage)
    }

    /// Sets player chat message with timeout
    pub fn set_chat_bubble(&mut self, message: String) {
        self.chat_bubble = Some(message);
        self.last_update = Utc::now();
        
        // Note: In a real implementation, you'd set a timer to clear the chat bubble
    }

    /// Updates player equipment
    pub fn equip_item(&mut self, slot: &str, item_id: String) -> Result<(), PlayerPluginError> {
        match slot {
            "weapon" => self.equipment.weapon = Some(item_id),
            "armor" => {
                if !self.equipment.armor.contains(&item_id) {
                    self.equipment.armor.push(item_id);
                }
            },
            _ => {
                return Err(PlayerPluginError::InvalidPlayerData(
                    format!("Invalid equipment slot: {}", slot)
                ));
            }
        }
        
        self.last_update = Utc::now();
        Ok(())
    }
}

impl GorcObject for GorcPlayer {
    fn type_name(&self) -> &'static str {
        "GorcPlayer"
    }
    
    fn position(&self) -> Vec3 {
        self.position
    }
    
    fn get_priority(&self, observer_pos: Vec3) -> ReplicationPriority {
        let distance = self.position.distance(observer_pos);
        if distance < 25.0 {
            ReplicationPriority::Critical
        } else if distance < 100.0 {
            ReplicationPriority::High
        } else if distance < 500.0 {
            ReplicationPriority::Normal
        } else {
            ReplicationPriority::Low
        }
    }
    
    fn serialize_for_layer(&self, layer: &ReplicationLayer) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut data = serde_json::Map::new();
        
        for property in &layer.properties {
            match property.as_str() {
                // Critical layer properties
                "position" => {
                    data.insert("position".to_string(), serde_json::to_value(&self.position)?);
                }
                "velocity" => {
                    data.insert("velocity".to_string(), serde_json::to_value(&self.velocity)?);
                }
                "health" => {
                    data.insert("health".to_string(), serde_json::to_value(&self.health)?);
                }
                "player_id" => {
                    data.insert("player_id".to_string(), serde_json::to_value(&self.player_id)?);
                }
                
                // Detailed layer properties
                "combat" => {
                    data.insert("combat".to_string(), serde_json::to_value(&self.combat)?);
                }
                "equipment" => {
                    data.insert("equipment".to_string(), serde_json::to_value(&self.equipment)?);
                }
                "rotation" => {
                    data.insert("rotation".to_string(), serde_json::to_value(self.rotation)?);
                }
                "movement_state" => {
                    data.insert("movement_state".to_string(), serde_json::to_value(self.movement_state)?);
                }
                
                // Social layer properties
                "display_name" => {
                    data.insert("display_name".to_string(), serde_json::to_value(&self.display_name)?);
                }
                "emote_state" => {
                    data.insert("emote_state".to_string(), serde_json::to_value(&self.emote_state)?);
                }
                "chat_bubble" => {
                    data.insert("chat_bubble".to_string(), serde_json::to_value(&self.chat_bubble)?);
                }
                "status_flags" => {
                    data.insert("status_flags".to_string(), serde_json::to_value(self.status_flags)?);
                }
                
                // Metadata layer properties
                "level" => {
                    data.insert("level".to_string(), serde_json::to_value(self.level)?);
                }
                "experience" => {
                    data.insert("experience".to_string(), serde_json::to_value(self.experience)?);
                }
                "player_class" => {
                    data.insert("player_class".to_string(), serde_json::to_value(&self.player_class)?);
                }
                "guild_info" => {
                    data.insert("guild_info".to_string(), serde_json::to_value(&self.guild_info)?);
                }
                "achievement_badges" => {
                    data.insert("achievement_badges".to_string(), serde_json::to_value(&self.achievement_badges)?);
                }
                
                _ => {} // Ignore unknown properties
            }
        }
        
        Ok(serde_json::to_vec(&data)?)
    }
    
    fn get_layers(&self) -> Vec<ReplicationLayer> {
        vec![
            // Critical Layer (Channel 0): Essential gameplay state - 60Hz, small range
            ReplicationLayer::new(
                0,
                25.0,  // 25 meter range
                60.0,  // 60 Hz update rate
                vec![
                    "position".to_string(),
                    "velocity".to_string(),
                    "health".to_string(),
                    "player_id".to_string(),
                ],
                CompressionType::Delta // Delta compression for position data
            ),
            
            // Detailed Layer (Channel 1): Visual and gameplay state - 30Hz, medium range
            ReplicationLayer::new(
                1,
                100.0, // 100 meter range
                30.0,  // 30 Hz update rate
                vec![
                    "combat".to_string(),
                    "equipment".to_string(),
                    "rotation".to_string(),
                    "movement_state".to_string(),
                ],
                CompressionType::Lz4 // Good compression for structured data
            ),
            
            // Social Layer (Channel 2): Chat and social interactions - 15Hz, wider range
            ReplicationLayer::new(
                2,
                200.0, // 200 meter range for chat visibility
                15.0,  // 15 Hz update rate
                vec![
                    "display_name".to_string(),
                    "emote_state".to_string(),
                    "chat_bubble".to_string(),
                    "status_flags".to_string(),
                ],
                CompressionType::High // High compression for text data
            ),
            
            // Metadata Layer (Channel 3): Character information - 2Hz, very wide range
            ReplicationLayer::new(
                3,
                1000.0, // 1km range for strategic information
                2.0,    // 2 Hz update rate (changes rarely)
                vec![
                    "level".to_string(),
                    "experience".to_string(),
                    "player_class".to_string(),
                    "guild_info".to_string(),
                    "achievement_badges".to_string(),
                ],
                CompressionType::High // Maximum compression for rarely-changing data
            ),
        ]
    }
    
    fn update_position(&mut self, new_position: Vec3) {
        self.last_position = self.position;
        self.position = new_position;
        self.last_update = Utc::now();
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    
    fn clone_object(&self) -> Box<dyn GorcObject> {
        Box::new(self.clone())
    }
}