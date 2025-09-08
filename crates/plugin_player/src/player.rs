use serde::{Deserialize, Serialize};
use horizon_event_system::{PlayerId, Vec3, GorcZoneData, impl_gorc_object};
use chrono::{DateTime, Utc};

/// Player critical data - high frequency updates (Zone 0)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerCriticalData {
    pub position: Vec3,
    pub velocity: Vec3,
    pub health: f32,
}

impl GorcZoneData for PlayerCriticalData {
    fn zone_type_name() -> &'static str {
        "PlayerCriticalData"
    }
}

/// Player detailed data - medium frequency updates (Zone 1)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerDetailedData {
    pub movement_state: String,
    pub level: u32,
}

impl GorcZoneData for PlayerDetailedData {
    fn zone_type_name() -> &'static str {
        "PlayerDetailedData"
    }
}

/// Player social data - social interactions (Zone 2)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSocialData {
    pub chat_bubble: Option<String>,
    pub name: String,
}

impl GorcZoneData for PlayerSocialData {
    fn zone_type_name() -> &'static str {
        "PlayerSocialData"
    }
}

/// Type-based GORC Player using proper zone structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GorcPlayer {
    pub player_id: PlayerId,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_update: DateTime<Utc>,
    /// Zone 0: Critical player state (25m, 60Hz)
    pub critical_data: PlayerCriticalData,
    /// Zone 1: Detailed state (100m, 30Hz) 
    pub detailed_data: PlayerDetailedData,
    /// Zone 2: Social data (200m, 15Hz)
    pub social_data: PlayerSocialData,
}

impl GorcPlayer {
    pub fn new(player_id: PlayerId, name: String, position: Vec3) -> Self {
        Self {
            player_id,
            last_update: Utc::now(),
            critical_data: PlayerCriticalData {
                position,
                velocity: Vec3::new(0.0, 0.0, 0.0),
                health: 100.0,
            },
            detailed_data: PlayerDetailedData {
                movement_state: "idle".to_string(),
                level: 1,
            },
            social_data: PlayerSocialData {
                chat_bubble: None,
                name,
            },
        }
    }

    pub fn set_chat_bubble(&mut self, message: String) {
        self.social_data.chat_bubble = Some(message);
        self.last_update = Utc::now();
    }

    pub fn validate_and_apply_movement(&mut self, new_position: Vec3, velocity: Vec3) -> Result<(), String> {
        // Basic validation - ensure movement isn't too large
        let distance = ((new_position.x - self.critical_data.position.x).powi(2) + 
                       (new_position.y - self.critical_data.position.y).powi(2) + 
                       (new_position.z - self.critical_data.position.z).powi(2)).sqrt();
        
        if distance > 100.0 {
            return Err("Movement too large".to_string());
        }

        self.critical_data.position = new_position;
        self.critical_data.velocity = velocity;
        self.last_update = Utc::now();
        Ok(())
    }

    pub fn perform_attack(&mut self, _target_position: Vec3) -> Result<f32, String> {
        // Simple attack logic
        let damage = 25.0;
        self.last_update = Utc::now();
        Ok(damage)
    }

    /// Get current position (for compatibility)
    pub fn position(&self) -> Vec3 {
        self.critical_data.position
    }
}

// Implement the type-based GorcObject using proper zone structure
impl_gorc_object! {
    GorcPlayer {
        0 => critical_data: PlayerCriticalData,  // 25m range, 60Hz - position, velocity, health
        1 => detailed_data: PlayerDetailedData,  // 100m range, 30Hz - level, movement_state  
        2 => social_data: PlayerSocialData,      // 200m range, 15Hz - chat_bubble, name
    }
}