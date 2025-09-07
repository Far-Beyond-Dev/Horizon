//! # Player-specific Event Definitions
//!
//! This module defines all events related to player actions and state changes.
//! These events flow through the complete GORC chain from client to server and back.

use horizon_event_system::{PlayerId, Vec3};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Event sent by client to request player movement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMoveRequest {
    /// Player requesting the move
    pub player_id: PlayerId,
    /// New position requested
    pub new_position: Vec3,
    /// New velocity vector
    pub velocity: Vec3,
    /// Movement state flags (running, jumping, etc.)
    pub movement_state: u32,
    /// Client timestamp
    pub client_timestamp: DateTime<Utc>,
}

/// Event sent by client to request attack action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAttackRequest {
    /// Player performing the attack
    pub player_id: PlayerId,
    /// Target position or direction
    pub target_position: Vec3,
    /// Attack type identifier
    pub attack_type: String,
    /// Client timestamp
    pub client_timestamp: DateTime<Utc>,
}

/// Event sent by client for interaction with objects/NPCs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInteractRequest {
    /// Player performing the interaction
    pub player_id: PlayerId,
    /// Target object or entity ID
    pub target_id: String,
    /// Type of interaction
    pub interaction_type: String,
    /// Additional interaction data
    pub interaction_data: serde_json::Value,
}

/// Event sent by client for chat messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatRequest {
    /// Player sending the message
    pub player_id: PlayerId,
    /// Chat message content
    pub message: String,
    /// Chat channel (global, local, guild, etc.)
    pub channel: String,
    /// Target player ID for private messages (optional)
    pub target_player: Option<PlayerId>,
}

/// Server event for confirmed position update (replicated to clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPositionUpdate {
    /// Player whose position was updated
    pub player_id: PlayerId,
    /// Confirmed position
    pub position: Vec3,
    /// Current velocity
    pub velocity: Vec3,
    /// Movement state flags
    pub movement_state: u32,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
    /// Sequence number for ordering
    pub sequence: u64,
}

/// Server event for combat action result (replicated to clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerCombatEvent {
    /// Player who performed the action
    pub attacker_id: PlayerId,
    /// Target of the action (if applicable)
    pub target_id: Option<PlayerId>,
    /// Damage dealt (if applicable)
    pub damage: Option<f32>,
    /// Attack position
    pub attack_position: Vec3,
    /// Combat event type
    pub event_type: String,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}

/// Server event for player health changes (replicated to clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerHealthUpdate {
    /// Player whose health changed
    pub player_id: PlayerId,
    /// Current health
    pub current_health: f32,
    /// Maximum health
    pub max_health: f32,
    /// Health change amount (positive for healing, negative for damage)
    pub health_change: f32,
    /// Source of the health change
    pub source: String,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}

/// Server event for equipment changes (replicated to nearby clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerEquipmentUpdate {
    /// Player whose equipment changed
    pub player_id: PlayerId,
    /// Equipment slot that changed
    pub slot: String,
    /// New item ID (None if unequipped)
    pub item_id: Option<String>,
    /// Visual appearance data
    pub appearance_data: serde_json::Value,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}

/// Server event for chat message broadcast (replicated to relevant clients)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatBroadcast {
    /// Player who sent the message
    pub player_id: PlayerId,
    /// Player's display name
    pub player_name: String,
    /// Chat message content
    pub message: String,
    /// Chat channel
    pub channel: String,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}

/// Server event for player status changes (level up, achievements, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStatusUpdate {
    /// Player whose status changed
    pub player_id: PlayerId,
    /// Status type (level, achievement, etc.)
    pub status_type: String,
    /// Old value
    pub old_value: serde_json::Value,
    /// New value
    pub new_value: serde_json::Value,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}

/// Event for player disconnection cleanup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDisconnectionEvent {
    /// Player who disconnected
    pub player_id: PlayerId,
    /// Disconnect reason
    pub reason: String,
    /// Final position before disconnect
    pub last_position: Vec3,
    /// Session duration in seconds
    pub session_duration: u64,
    /// Server timestamp
    pub server_timestamp: DateTime<Utc>,
}