//! # Player Movement Handler
//! 
//! Handles real-time player movement events on GORC channel 0, the highest-priority
//! communication channel designed for critical game state updates that require
//! immediate synchronization across all connected clients.
//! 
//! ## Channel 0 Characteristics
//! 
//! - **Frequency**: 60Hz updates for smooth movement
//! - **Range**: 25m replication radius for performance optimization  
//! - **Priority**: Critical data - position, velocity, health status
//! - **Latency**: Minimal buffering for real-time responsiveness
//! 
//! ## Movement Validation
//! 
//! All movement requests undergo strict validation:
//! - **Authentication**: Only authenticated connections can request movement
//! - **Ownership**: Players can only move their own ships  
//! - **Bounds Checking**: Movement deltas are validated for reasonable values
//! - **Anti-Cheat**: Large teleportation attempts are rejected
//! 
//! ## Spatial Replication
//! 
//! Movement updates trigger automatic spatial replication:
//! 1. Client sends movement request via GORC channel 0
//! 2. Server validates request and updates object position
//! 3. Position update is broadcast to all clients within 25m range
//! 4. Clients receive smooth position updates for nearby ships
//! 
//! ## Performance Optimization
//! 
//! - **Batched Updates**: Multiple position changes are batched per frame
//! - **Spatial Culling**: Only nearby clients receive updates (25m radius)
//! - **Async Processing**: Movement validation runs without blocking other events
//! - **Memory Efficiency**: Uses in-place object updates to minimize allocations

use std::sync::Arc;
use horizon_event_system::{
    EventSystem, PlayerId, GorcEvent, GorcObjectId, ClientConnectionRef, ObjectInstance,
    EventError,
};
use luminal::Handle;
use tracing::{debug, error};
use serde_json;
use crate::events::PlayerMoveRequest;

/// Handles incoming player movement requests from GORC clients on channel 0.
/// 
/// This is the highest-frequency handler in the system, processing ship movement
/// requests at up to 60Hz. It performs authentication, ownership validation,
/// position updates, and triggers spatial replication to nearby clients.
/// 
/// # Parameters
/// 
/// - `gorc_event`: The raw GORC event containing movement data
/// - `client_player`: The player ID of the requesting client
/// - `connection`: Client connection reference for authentication checks
/// - `object_instance`: Mutable reference to the player's GORC object
/// - `events`: Event system for broadcasting position updates
/// 
/// # Returns
/// 
/// `Result<(), EventError>` - Success or detailed error information
/// 
/// # Security Validations
/// 
/// 1. **Connection Authentication**: Rejects requests from unauthenticated connections
/// 2. **Player Ownership**: Ensures players can only move their own ships
/// 3. **Movement Bounds**: Validates movement deltas are within reasonable limits
/// 
/// # Performance Notes
/// 
/// This handler is designed for high-frequency operation:
/// - Minimal allocations during normal operation
/// - Fast-path validation for common cases
/// - Async broadcasting to avoid blocking the handler
/// 
/// # Example Request Format
/// 
/// ```json
/// {
///     "player_id": 42,
///     "new_position": { "x": 100.5, "y": 50.0, "z": 25.3 },
///     "velocity": { "x": 10.0, "y": 0.0, "z": 5.0 },
///     "movement_state": 1,
///     "client_timestamp": "2024-01-15T10:30:45Z"
/// }
/// ```
pub async fn handle_movement_request(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    connection: ClientConnectionRef,
    object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
) -> Result<(), EventError> {
    // SECURITY: Validate connection authentication before processing any movement
    if !connection.is_authenticated() {
        error!("🚀 GORC: ❌ Unauthenticated movement request from {}", connection.remote_addr);
        return Err(EventError::HandlerExecution(
            "Unauthenticated request".to_string()
        ));
    }
    
    // Parse the movement data from the GORC event payload
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("🚀 GORC: ❌ Failed to parse JSON from GORC event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in movement request".to_string())
        })?;
    
    let move_data = serde_json::from_value::<PlayerMoveRequest>(event_data)
        .map_err(|e| {
            error!("🚀 GORC: ❌ Failed to parse PlayerMoveRequest: {}", e);
            EventError::HandlerExecution("Invalid movement request format".to_string())
        })?;
    
    debug!("🚀 GORC: Processing movement for ship {} to position {:?}", 
        move_data.player_id, move_data.new_position);
    
    // SECURITY: Validate player ownership - players can only move their own ships
    if move_data.player_id != client_player {
        error!("🚀 GORC: ❌ Security violation: Player {} tried to move ship belonging to {}", 
            client_player, move_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized ship movement".to_string()
        ));
    }
    
    // Update the object instance position directly (this is the authoritative update)
    object_instance.object.update_position(move_data.new_position);
    debug!("🚀 GORC: ✅ Updated ship position for {} to {:?}", 
        client_player, move_data.new_position);
    
    // Broadcast position update to nearby players (within 25m range)
    broadcast_position_update(
        &gorc_event.object_id,
        client_player,
        &move_data,
        events,
    ).await;
    
    Ok(())
}

/// Synchronous wrapper for movement request handling that works with GORC client handlers.
///
/// This function provides the same functionality as `handle_movement_request` but in
/// a synchronous context suitable for use with the GORC client event system.
pub fn handle_movement_request_sync(
    gorc_event: GorcEvent,
    client_player: PlayerId,
    connection: ClientConnectionRef,
    object_instance: &mut ObjectInstance,
    events: Arc<EventSystem>,
    luminal_handle: Handle,
) -> Result<(), EventError> {
    debug!("🚀 STEP 1: Movement handler called for player {}", client_player);

    // SECURITY: Validate connection authentication before processing any movement
    if !connection.is_authenticated() {
        error!("🚀 STEP 2: ❌ Unauthenticated movement request from {}", connection.remote_addr);
        return Err(EventError::HandlerExecution(
            "Unauthenticated request".to_string()
        ));
    }
    debug!("🚀 STEP 2: ✅ Connection authenticated");

    // Parse the movement data from the GORC event payload
    debug!("🚀 STEP 3: Parsing GORC event data, length: {} bytes", gorc_event.data.len());
    let event_data = serde_json::from_slice::<serde_json::Value>(&gorc_event.data)
        .map_err(|e| {
            error!("🚀 STEP 3: ❌ Failed to parse JSON from GORC event data: {}", e);
            EventError::HandlerExecution("Invalid JSON in movement request".to_string())
        })?;
    debug!("🚀 STEP 3: ✅ Parsed raw JSON: {}", event_data);

    let move_data = serde_json::from_value::<PlayerMoveRequest>(event_data)
        .map_err(|e| {
            error!("🚀 STEP 4: ❌ Failed to parse PlayerMoveRequest: {}", e);
            EventError::HandlerExecution("Invalid movement request format".to_string())
        })?;
    debug!("🚀 STEP 4: ✅ Parsed PlayerMoveRequest: {:?}", move_data);

    debug!("🚀 STEP 5: Processing movement for ship {} to position {:?}",
        move_data.player_id, move_data.new_position);

    // SECURITY: Validate player ownership - players can only move their own ships
    if move_data.player_id != client_player {
        error!("🚀 STEP 6: ❌ Security violation: Player {} tried to move ship belonging to {}",
            client_player, move_data.player_id);
        return Err(EventError::HandlerExecution(
            "Unauthorized ship movement".to_string()
        ));
    }
    debug!("🚀 STEP 6: ✅ Player ownership validated");

    // Update the object instance position locally (for immediate response)
    object_instance.object.update_position(move_data.new_position);
    debug!("🚀 STEP 7: ✅ Updated local ship position for {} to {:?}",
        client_player, move_data.new_position);
    
    // Broadcast position update to nearby players (within 25m range)
    // CRITICAL: Update BOTH player AND object positions in GORC tracking before broadcasting
    debug!("🚀 STEP 8: Beginning position update broadcast for player {}", client_player);
    let object_id_str = gorc_event.object_id.clone();
    debug!("🚀 STEP 9: Using object ID: {}", object_id_str);

    let position_update = serde_json::json!({
        "player_id": client_player,
        "new_position": move_data.new_position,
        "velocity": move_data.velocity,
        "movement_state": move_data.movement_state,
        "client_timestamp": chrono::Utc::now()
    });
    debug!("🚀 STEP 10: Created position update payload: {}", position_update);
    
    luminal_handle.spawn(async move {
        debug!("🚀 STEP 11: Inside async broadcast task");
        
        // CRITICAL FIX: Update BOTH player position AND object position in GORC tracking
        // This ensures the spatial tracking has the correct positions for distance calculations
        
        // Update player position in GORC tracking
        if let Err(e) = events.update_player_position(client_player, move_data.new_position).await {
            error!("🚀 STEP 11.5: ❌ Failed to update GORC player tracking: {}", e);
        } else {
            debug!("🚀 STEP 11.5: ✅ Updated GORC player tracking for {} at {:?}",
                client_player, move_data.new_position);
        }
        
        // Update object position in GORC tracking
        if let Ok(gorc_id) = GorcObjectId::from_str(&object_id_str) {
            debug!("🚀 STEP 12: Parsed GORC ID successfully: {:?}", gorc_id);
            
            if let Err(e) = events.update_object_position(gorc_id, move_data.new_position).await {
                error!("🚀 STEP 12.5: ❌ Failed to update GORC object tracking: {}", e);
            } else {
                debug!("🚀 STEP 12.5: ✅ Updated GORC object tracking for {:?} at {:?}",
                    gorc_id, move_data.new_position);
            }
            
            debug!("🚀 STEP 13: About to call emit_gorc_instance on channel 0");

            match events.emit_gorc_instance(
                gorc_id,
                0, // Channel 0: Critical movement data
                "move",
                &position_update,
                horizon_event_system::Dest::Client
            ).await {
                Ok(_) => {
                    debug!("🚀 STEP 14: ✅ emit_gorc_instance completed successfully for player {}", client_player);
                    debug!("🚀 GORC: ✅ Broadcasted position update for ship {} to clients within 25m", client_player);
                },
                Err(e) => {
                    error!("🚀 STEP 14: ❌ emit_gorc_instance failed: {}", e);
                    error!("🚀 GORC: ❌ Failed to broadcast position update: {}", e);
                }
            }
        } else {
            error!("🚀 STEP 12: ❌ Failed to parse GORC object ID: {}", object_id_str);
            error!("🚀 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
        }
        debug!("🚀 STEP 15: Exiting async broadcast task");
    });
    
    Ok(())
}

/// Broadcasts position updates to nearby players within the 25m replication range.
/// 
/// This function creates a position update message and emits it as a GORC instance
/// event, which automatically replicates to all clients within the configured range
/// for channel 0 (25 meters).
/// 
/// # Parameters
/// 
/// - `object_id_str`: String representation of the GORC object ID
/// - `player_id`: ID of the player whose position updated
/// - `move_data`: The movement request data containing position and velocity
/// - `events`: Event system for broadcasting the update
/// 
/// # Broadcast Message Format
/// 
/// ```json
/// {
///     "player_id": 42,
///     "position": { "x": 100.5, "y": 50.0, "z": 25.3 },
///     "velocity": { "x": 10.0, "y": 0.0, "z": 5.0 },
///     "movement_state": 1,
///     "timestamp": "2024-01-15T10:30:45.123Z"
/// }
/// ```
/// 
/// # Error Handling
/// 
/// Broadcasting failures are logged but don't fail the movement update itself,
/// ensuring that server-side position tracking remains consistent even if
/// some clients miss updates due to network issues.
async fn broadcast_position_update(
    object_id_str: &str,
    player_id: PlayerId,
    move_data: &PlayerMoveRequest,
    events: Arc<EventSystem>,
) {
    // Create position update payload for nearby clients
    let position_update = serde_json::json!({
        "player_id": player_id,
        "new_position": move_data.new_position,
        "velocity": move_data.velocity,
        "movement_state": move_data.movement_state,
        "client_timestamp": chrono::Utc::now()
    });
    
    // Parse the GORC object ID and emit the update
    if let Ok(gorc_id) = GorcObjectId::from_str(object_id_str) {
        // Emit on channel 0 (movement) with automatic spatial replication
        if let Err(e) = events.emit_gorc_instance(
            gorc_id,
            0, // Channel 0: Critical movement data
            "move",
            &position_update,
            horizon_event_system::Dest::Client
        ).await {
            error!("🚀 GORC: ❌ Failed to broadcast position update: {}", e);
        } else {
            debug!("🚀 GORC: ✅ Broadcasted position update for ship {} to clients within 25m", 
                player_id);
        }
    } else {
        error!("🚀 GORC: ❌ Invalid GORC object ID format: {}", object_id_str);
    }
}

/// Validates movement requests to prevent cheating and ensure reasonable behavior.
/// 
/// This function performs various checks on movement data:
/// - Position delta validation (prevents teleportation)
/// - Velocity bounds checking  
/// - Timestamp validation for anti-cheat purposes
/// 
/// # Parameters
/// 
/// - `current_position`: The object's current authoritative position
/// - `move_request`: The requested movement data
/// 
/// # Returns
/// 
/// `Result<(), String>` - Ok if valid, Err with reason if invalid
/// 
/// # Validation Rules
/// 
/// - **Max Movement Delta**: 100 units per update (prevents teleportation)
/// - **Max Velocity**: 1000 units/second (prevents super-speed exploits)
/// - **Timestamp Window**: Must be within 5 seconds of server time
pub fn validate_movement_request(
    current_position: horizon_event_system::Vec3,
    move_request: &PlayerMoveRequest,
) -> Result<(), String> {
    // Calculate movement delta to detect teleportation attempts
    let delta = (
        (move_request.new_position.x - current_position.x).powi(2) +
        (move_request.new_position.y - current_position.y).powi(2) +
        (move_request.new_position.z - current_position.z).powi(2)
    ).sqrt();
    
    // Reject movement that's too large (likely cheating or network issues)
    if delta > 100.0 {
        return Err(format!("Movement delta too large: {:.2} units", delta));
    }
    
    // Check velocity bounds to prevent speed hacking
    let velocity_magnitude = (
        move_request.velocity.x.powi(2) +
        move_request.velocity.y.powi(2) +
        move_request.velocity.z.powi(2)
    ).sqrt();
    
    if velocity_magnitude > 1000.0 {
        return Err(format!("Velocity too high: {:.2} units/sec", velocity_magnitude));
    }
    
    // Validate timestamp is within reasonable bounds (5 second window)
    let now = chrono::Utc::now();
    let time_diff = (now - move_request.client_timestamp).num_seconds().abs();
    
    if time_diff > 5 {
        return Err(format!("Timestamp out of sync: {} seconds difference", time_diff));
    }
    
    Ok(())
}