//! # Combat Handler
//! 
//! Manages player combat interactions on GORC channel 1, including weapon firing,
//! attack validation, and combat event replication to nearby ships within
//! the 500-meter combat awareness range.
//! 
//! ## Channel 1 Characteristics
//! 
//! - **Purpose**: Combat events - weapon fire, explosions, damage dealing
//! - **Range**: 500m replication radius for tactical awareness
//! - **Frequency**: Event-driven (not continuous like movement)
//! - **Priority**: High priority for combat responsiveness
//! 
//! ## Combat System Design
//! 
//! The combat system follows a "fire and replicate" model:
//! 1. Player initiates attack via client interface
//! 2. Server validates attack request and authorization
//! 3. Server broadcasts weapon fire to all ships within 500m
//! 4. Clients handle visual effects, damage calculations, and UI updates
//! 
//! ## Security Model
//! 
//! Combat events require strict validation:
//! - **Player Ownership**: Only ship owners can fire weapons
//! - **Rate Limiting**: Prevents rapid-fire exploits (future enhancement)
//! - **Range Validation**: Ensures weapon fire targets are within reasonable range
//! - **Ammunition Tracking**: Validates available ammunition (future enhancement)
//! 
//! ## Weapon Types
//! 
//! The system supports multiple weapon types with different characteristics:
//! - **"laser"**: High-precision energy weapons with instant hit-scan
//! - **"missile"**: Guided projectiles with area-of-effect damage
//! - **"plasma"**: Energy bolts with travel time and splash damage
//! - **"kinetic"**: Physical projectiles with ballistic trajectories

use std::sync::Arc;
use horizon_event_system::{
    EventSystem, PlayerId, GorcEvent, GorcObjectId, ClientConnectionRef, ObjectInstance,
    EventError,
};
use tracing::{debug, error};
use serde_json;
use crate::events::PlayerAttackRequest;

/// Handles combat requests from players on GORC channel 1.
/// 
/// This handler processes weapon fire requests, validates player authorization,
/// and broadcasts combat events to nearby ships for tactical awareness and
/// visual effect replication.
/// 
/// # Parameters
/// 
/// - `gorc_event`: The GORC event containing attack data
/// - `client_player`: ID of the player initiating the attack
/// - `_connection`: Client connection (unused but available for future rate limiting)
/// - `_object_instance`: Player's object instance (unused but available for state checks)
/// - `events`: Event system for broadcasting combat events
/// 
/// # Returns
/// 
/// `Result<(), EventError>` - Success or detailed error information
/// 
/// # Combat Flow
/// 
/// 1. Parse attack request from GORC event data
/// 2. Validate player owns the attacking ship
/// 3. Create weapon fire broadcast message
/// 4. Emit to all ships within 500m range on channel 1
/// 5. Log successful combat event for monitoring
/// 
/// # Example Attack Request
/// 
/// ```json
/// {
///     "player_id": 42,
///     "target_position": { "x": 150.0, "y": 75.0, "z": -20.0 },
///     "attack_type": "laser",
///     "client_timestamp": "2024-01-15T10:30:45Z"
/// }
/// ```
/// 
/// # Broadcast Message
/// 
/// ```json
/// {
///     "attacker_player": 42,
///     "weapon_type": "laser", 
///     "target_position": { "x": 150.0, "y": 75.0, "z": -20.0 },
///     "fire_timestamp": "2024-01-15T10:30:45.123Z"
/// }
/// ```
pub async fn handle_combat_request(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
) -> Result<(), EventError> {
    debug!("⚡ GORC: Received client combat request from ship {}: {:?}", 
        client_player, gorc_event);
    
    // Parse attack data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("⚡ GORC: ❌ Failed to parse JSON from GORC combat event: {}", e);
            EventError::HandlerExecution("Invalid JSON in combat request".to_string())
        })?;
    
    let attack_data = serde_json::from_value::<PlayerAttackRequest>(event_data)
        .map_err(|e| {
            error!("⚡ GORC: ❌ Failed to parse PlayerAttackRequest: {}", e);
            EventError::HandlerExecution("Invalid attack request format".to_string())
        })?;
    
    debug!("⚡ GORC: Ship {} fires {} at {:?}", 
        attack_data.player_id, attack_data.attack_type, attack_data.target_position);
    
    // SECURITY: Validate player ownership - only ship owners can fire weapons
    if attack_data.player_id != client_player {
        error!("⚡ GORC: ❌ Security violation: Player {} tried to fire weapons as {}", 
            client_player, attack_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized weapon fire".to_string()
        ));
    }
    
    // Broadcast weapon fire event to nearby ships
    broadcast_weapon_fire(
        &gorc_event.object_id,
        &attack_data,
        events,
    ).await;
    
    Ok(())
}

/// Synchronous wrapper for combat request handling that works with GORC client handlers.
///
/// This function provides the same functionality as `handle_combat_request` but in
/// a synchronous context suitable for use with the GORC client event system.
pub fn handle_combat_request_sync(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    _connection: ClientConnectionRef,
    _object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
) -> Result<(), EventError> {
    debug!("⚡ GORC: Received client combat request from ship {}: {:?}", 
        client_player, gorc_event);
    
    // Parse attack data from GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("⚡ GORC: ❌ Failed to parse JSON from GORC combat event: {}", e);
            EventError::HandlerExecution("Invalid JSON in combat request".to_string())
        })?;
    
    let attack_data = serde_json::from_value::<PlayerAttackRequest>(event_data)
        .map_err(|e| {
            error!("⚡ GORC: ❌ Failed to parse PlayerAttackRequest: {}", e);
            EventError::HandlerExecution("Invalid attack request format".to_string())
        })?;
    
    debug!("⚡ GORC: Ship {} fires {} at {:?}", 
        attack_data.player_id, attack_data.attack_type, attack_data.target_position);
    
    // SECURITY: Validate player ownership - only ship owners can fire weapons
    if attack_data.player_id != client_player {
        error!("⚡ GORC: ❌ Security violation: Player {} tried to fire weapons as {}", 
            client_player, attack_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized weapon fire".to_string()
        ));
    }
    
    // Broadcast weapon fire event to nearby ships
    let object_id_str = gorc_event.object_id.clone();
    let weapon_fire = serde_json::json!({
        "attacker_player": attack_data.player_id,
        "weapon_type": attack_data.attack_type,
        "target_position": attack_data.target_position,
        "fire_timestamp": chrono::Utc::now()
    });
    
    tokio::spawn(async move {
        if let Ok(gorc_id) = GorcObjectId::from_str(&object_id_str) {
            if let Err(e) = events.emit_gorc_instance(
                gorc_id, 
                1, // Channel 1: Combat events
                "weapon_fire", 
                &weapon_fire, 
                horizon_event_system::Dest::Client
            ).await {
                error!("⚡ GORC: ❌ Failed to broadcast weapon fire: {}", e);
            } else {
                debug!("⚡ GORC: ✅ Broadcasting weapon fire from ship {} to ships within 500m", 
                    attack_data.player_id);
            }
        } else {
            error!("⚡ GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
        }
    });
    
    Ok(())
}

/// Broadcasts weapon fire events to all ships within 500m combat range.
/// 
/// This function creates a standardized weapon fire message and emits it
/// via the GORC instance event system, which automatically handles spatial
/// replication to nearby clients.
/// 
/// # Parameters
/// 
/// - `object_id_str`: String representation of the firing ship's GORC object ID  
/// - `attack_data`: The validated attack request data
/// - `events`: Event system for broadcasting
/// 
/// # Combat Awareness Range
/// 
/// The 500m range ensures that:
/// - Ships have tactical awareness of nearby combat
/// - Visual and audio effects are displayed at appropriate distances
/// - Combat doesn't spam distant players with irrelevant events
/// - Network bandwidth is conserved for relevant combat data
/// 
/// # Message Structure
/// 
/// The broadcast message includes:
/// - **attacker_player**: ID of the ship that fired
/// - **weapon_type**: Type of weapon used (affects client-side effects)
/// - **target_position**: Where the weapon was aimed
/// - **fire_timestamp**: Precise timing for effect synchronization
async fn broadcast_weapon_fire(
    object_id_str: &str,
    attack_data: &PlayerAttackRequest,
    events: Arc<EventSystem>,
) {
    // Create weapon fire broadcast payload
    let weapon_fire = serde_json::json!({
        "attacker_player": attack_data.player_id,
        "weapon_type": attack_data.attack_type,
        "target_position": attack_data.target_position,
        "fire_timestamp": chrono::Utc::now()
    });
    
    // Parse GORC object ID and emit the combat event
    if let Ok(gorc_id) = GorcObjectId::from_str(object_id_str) {
        // Emit on channel 1 (combat) with 500m replication range
        if let Err(e) = events.emit_gorc_instance(
            gorc_id, 
            1, // Channel 1: Combat events
            "weapon_fire", 
            &weapon_fire, 
            horizon_event_system::Dest::Client
        ).await {
            error!("⚡ GORC: ❌ Failed to broadcast weapon fire: {}", e);
        } else {
            debug!("⚡ GORC: ✅ Broadcasting weapon fire from ship {} to ships within 500m", 
                attack_data.player_id);
        }
    } else {
        error!("⚡ GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
}

/// Validates combat requests to prevent exploits and ensure fair play.
/// 
/// This function performs security and gameplay validation:
/// - Weapon type verification
/// - Target range validation  
/// - Rate limiting (future enhancement)
/// - Ammunition checks (future enhancement)
/// 
/// # Parameters
/// 
/// - `attack_data`: The attack request to validate
/// - `_current_position`: Current position of the attacking ship (for range checks)
/// 
/// # Returns
/// 
/// `Result<(), String>` - Ok if valid, Err with reason if invalid
/// 
/// # Validation Rules
/// 
/// - **Valid Weapon Types**: Must be one of the supported weapon systems
/// - **Target Range**: Must be within maximum weapon range
/// - **Rate Limiting**: Enforces cooldown between weapon fire (future)
/// - **Ammunition**: Validates available ammunition (future)
pub fn validate_combat_request(
    attack_data: &PlayerAttackRequest,
    _current_position: horizon_event_system::Vec3,
) -> Result<(), String> {
    // Validate weapon type is supported
    let valid_weapons = ["laser", "missile", "plasma", "kinetic"];
    if !valid_weapons.contains(&attack_data.attack_type.as_str()) {
        return Err(format!("Invalid weapon type: {}", attack_data.attack_type));
    }
    
    // Future enhancements:
    // - Range validation based on weapon type
    // - Rate limiting per player
    // - Ammunition tracking
    // - Energy/resource consumption
    
    Ok(())
}

/// Calculates combat damage based on weapon type, distance, and ship characteristics.
/// 
/// This function implements the core damage calculation system:
/// - Different weapon types have different damage profiles
/// - Distance affects damage for some weapon types
/// - Ship armor and shields modify final damage
/// 
/// # Parameters
/// 
/// - `weapon_type`: Type of weapon fired
/// - `distance`: Distance from attacker to target
/// - `_target_armor`: Target ship's armor rating (future enhancement)
/// - `_target_shields`: Target ship's shield strength (future enhancement)
/// 
/// # Returns
/// 
/// `f32` - Final damage amount to be applied
/// 
/// # Weapon Damage Profiles
/// 
/// - **Laser**: 50 base damage, no distance falloff, instant hit
/// - **Missile**: 75 base damage, 10% falloff per 100m, guided
/// - **Plasma**: 60 base damage, 15% falloff per 100m, area effect  
/// - **Kinetic**: 40 base damage, no falloff, ballistic trajectory
pub fn calculate_damage(
    weapon_type: &str,
    distance: f32,
    _target_armor: f32,
    _target_shields: f32,
) -> f32 {
    let base_damage = match weapon_type {
        "laser" => 50.0,     // High-precision energy weapon
        "missile" => 75.0,   // Heavy guided projectile
        "plasma" => 60.0,    // Energy bolt with splash
        "kinetic" => 40.0,   // Physical projectile
        _ => 25.0,           // Unknown weapon fallback
    };
    
    // Apply distance falloff for certain weapon types
    let distance_modifier = match weapon_type {
        "laser" => 1.0,                                    // No falloff
        "kinetic" => 1.0,                                  // No falloff
        "missile" => (1.0 - (distance / 1000.0)).max(0.1), // 10% per 100m
        "plasma" => (1.0 - (distance / 666.67)).max(0.1),  // 15% per 100m  
        _ => 1.0,
    };
    
    // Future: Apply armor and shield modifiers
    // let armor_modifier = calculate_armor_reduction(target_armor);
    // let shield_modifier = calculate_shield_absorption(target_shields);
    
    base_damage * distance_modifier
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_weapon_validation_success() {
        let attack = PlayerAttackRequest {
            player_id: 1,
            target_position: horizon_event_system::Vec3::new(100.0, 0.0, 0.0),
            attack_type: "laser".to_string(),
            client_timestamp: chrono::Utc::now(),
        };
        
        let current_pos = horizon_event_system::Vec3::new(0.0, 0.0, 0.0);
        assert!(validate_combat_request(&attack, current_pos).is_ok());
    }
    
    #[test]
    fn test_weapon_validation_invalid_type() {
        let attack = PlayerAttackRequest {
            player_id: 1,
            target_position: horizon_event_system::Vec3::new(100.0, 0.0, 0.0),
            attack_type: "death_ray".to_string(), // Invalid weapon
            client_timestamp: chrono::Utc::now(),
        };
        
        let current_pos = horizon_event_system::Vec3::new(0.0, 0.0, 0.0);
        assert!(validate_combat_request(&attack, current_pos).is_err());
    }
    
    #[test]
    fn test_damage_calculation() {
        assert_eq!(calculate_damage("laser", 100.0, 0.0, 0.0), 50.0);
        assert_eq!(calculate_damage("kinetic", 200.0, 0.0, 0.0), 40.0);
        assert!(calculate_damage("missile", 500.0, 0.0, 0.0) < 75.0); // Should have falloff
    }
}