use serde::{Deserialize, Serialize};
use horizon_event_system::{PlayerId, Vec3};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerMoveRequest {
    pub player_id: PlayerId,
    pub new_position: Vec3,
    pub velocity: Vec3,
    pub movement_state: i32, // Changed from String to i32 to match test client
    pub client_timestamp: DateTime<Utc>, // Added field expected by test client
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerAttackRequest {
    pub player_id: PlayerId,
    pub target_position: Vec3,
    pub attack_type: String, // Added field expected by test client
    pub client_timestamp: DateTime<Utc>, // Added field expected by test client
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerChatRequest {
    pub player_id: PlayerId,
    pub message: String,
    pub channel: String, // Added field expected by test client
    pub target_player: Option<PlayerId>, // Added field expected by test client
}