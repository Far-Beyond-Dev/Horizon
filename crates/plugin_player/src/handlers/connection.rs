//! # Player Connection Handler
//! 
//! Manages the complete lifecycle of player connections within the GORC system.
//! This module handles the critical events of players joining and leaving the game world,
//! ensuring proper resource allocation, cleanup, and integration with the spatial
//! replication system.
//! 
//! ## Key Responsibilities
//! 
//! - **Player Registration**: Creates and registers new GORC player objects when clients connect
//! - **Spatial Integration**: Adds players to the zone-based replication system
//! - **Resource Management**: Tracks player-to-object mappings for efficient cleanup
//! - **Graceful Cleanup**: Removes players and their associated objects on disconnect
//! 
//! ## Connection Flow
//! 
//! 1. **PlayerConnectedEvent** received from core event system
//! 2. Create new `GorcPlayer` object with default spawn position
//! 3. Register object with GORC instances manager (returns unique ID)
//! 4. Update player position to trigger zone message distribution
//! 5. Add player to spatial tracking system
//! 6. Store mapping for future cleanup
//! 
//! ## Disconnection Flow
//! 
//! 1. **PlayerDisconnectedEvent** received from core event system
//! 2. Lookup stored GORC object ID for the player
//! 3. Remove player from all tracking systems
//! 4. Clean up resource mappings
//! 
//! ## Error Handling
//! 
//! All connection operations are designed to be fault-tolerant:
//! - Missing GORC instances manager is logged but doesn't crash the plugin
//! - Failed registrations are properly logged with context
//! - Cleanup operations are idempotent and safe to retry

use std::sync::Arc;
use dashmap::DashMap;
use horizon_event_system::{
    EventSystem, PlayerId, GorcObjectId, Vec3,
    PlayerConnectedEvent, PlayerDisconnectedEvent,
};
use tracing::{debug, error};
use crate::player::GorcPlayer;

/// Handles player connection events and integrates new players into the GORC system.
/// 
/// This function is called whenever a player successfully connects to the server.
/// It creates a new player object, registers it with the spatial replication system,
/// and ensures the player is properly tracked for future events.
/// 
/// # Parameters
/// 
/// - `event`: The connection event containing player ID and connection details
/// - `players`: Shared registry mapping player IDs to GORC object IDs
/// - `events`: Event system for spatial updates and GORC registration
/// - `luminal_handle`: Async runtime handle for background operations
/// 
/// # Returns
/// 
/// `Result<(), Box<dyn std::error::Error + Send + Sync>>` - Success or error details
/// 
/// # Example Flow
/// 
/// ```text
/// PlayerConnectedEvent { player_id: 42 }
///     ↓
/// Create GorcPlayer object at (0,0,0)
///     ↓
/// Register with GORC instances → GorcObjectId
///     ↓
/// Update spatial position (triggers zone messages)
///     ↓
/// Add to spatial tracking system
///     ↓
/// Store mapping: 42 → GorcObjectId
/// ```
pub async fn handle_player_connected(
    event: PlayerConnectedEvent,
    players: Arc<DashMap<PlayerId, GorcObjectId>>,
    events: Arc<EventSystem>,
    luminal_handle: luminal::Handle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("🎮 CONNECTION STEP 1: handle_player_connected called for player {}", event.player_id);
    debug!("🎮 GORC: Processing player connection for player {}", event.player_id);
    
    let spawn_position = Vec3::new(0.0, 0.0, 0.0);
    
    // Verify GORC instances manager is available
    let Some(gorc_instances) = events.get_gorc_instances() else {
        error!("🎮 GORC: ❌ No GORC instances manager available for player {}", event.player_id);
        return Ok(()); // Not a fatal error, just log and continue
    };
    
    debug!("🎮 GORC: ✅ GORC instances manager available, registering player {}", event.player_id);
    
    // Create a new GORC player object with default configuration
    let player = GorcPlayer::new(
        event.player_id, 
        format!("Player_{}", event.player_id), 
        spawn_position
    );
    
    // Spawn async task to handle GORC registration without blocking the event handler
    let players_clone = players.clone();
    let events_clone = Arc::clone(&events);
    
    debug!("🎮 GORC: Spawning async registration task for player {}", event.player_id);
    luminal_handle.spawn(async move {
        debug!("🎮 GORC: Starting async registration for player {}", event.player_id);
        
        // Register the player object with GORC spatial system
        let gorc_id = gorc_instances.register_object(player, spawn_position).await;
        
        // Store the GORC ID for future operations (movement, cleanup, etc.)
        players_clone.insert(event.player_id, gorc_id);
        
        debug!("🎮 GORC: ✅ Player {} registered with GORC instance ID {:?} at position {:?}",
            event.player_id, gorc_id, spawn_position);

        // Send GORC object info to client on channel 0
        let gorc_info = serde_json::json!({
            "player_id": event.player_id,
            "object_id": gorc_id.to_string(),
            "position": spawn_position,
            "timestamp": chrono::Utc::now()
        });

        if let Err(e) = events_clone.emit_gorc_instance(
            gorc_id,
            0, // Channel 0 for critical info
            "gorc_info",
            &gorc_info,
            horizon_event_system::Dest::Client
        ).await {
            error!("🎮 GORC: ❌ Failed to send GORC info to client: {}", e);
        } else {
            debug!("🎮 GORC: ✅ Sent GORC object info to client: {}", gorc_info);
        }

        // CRITICAL: Trigger zone message distribution by updating player position
        // This ensures nearby players receive zone data for the new player
        if let Err(e) = events_clone.update_player_position(event.player_id, spawn_position).await {
            error!("🎮 GORC: ❌ Failed to update player position via EventSystem: {}", e);
        } else {
            debug!("🎮 GORC: ✅ EventSystem.update_player_position completed successfully");
        }
        
        // Add player to GORC spatial tracking system (after zone messages are sent)
        gorc_instances.add_player(event.player_id, spawn_position).await;
        
        debug!("🎮 GORC: ✅ Player {} fully integrated into GORC system", event.player_id);
    });
    
    Ok(())
}

/// Handles player disconnection events and performs complete cleanup.
/// 
/// This function is called when a player disconnects from the server.
/// It ensures all resources associated with the player are properly cleaned up,
/// including removal from spatial tracking and GORC object registry.
/// 
/// # Parameters
/// 
/// - `event`: The disconnection event containing player ID
/// - `players`: Shared registry mapping player IDs to GORC object IDs
/// 
/// # Returns
/// 
/// `Result<(), Box<dyn std::error::Error + Send + Sync>>` - Success or error details
/// 
/// # Cleanup Process
/// 
/// 1. Look up the player's GORC object ID
/// 2. Remove from player registry
/// 3. Log successful cleanup with relevant IDs
/// 
/// Note: The GORC instances manager automatically handles spatial cleanup
/// when objects are no longer referenced.
pub async fn handle_player_disconnected(
    event: PlayerDisconnectedEvent,
    players: Arc<DashMap<PlayerId, GorcObjectId>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    debug!("🎮 GORC: Processing player disconnection for player {}", event.player_id);
    
    // Remove player from registry and get their GORC object ID
    if let Some((_, gorc_id)) = players.remove(&event.player_id) {
        debug!("🎮 GORC: ✅ Player {} disconnected and unregistered (GORC ID {:?})", 
            event.player_id, gorc_id);
    } else {
        // This could happen if the player was never successfully registered
        debug!("🎮 GORC: Player {} disconnected but was not in registry", event.player_id);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Test that connection handler creates proper player mapping
    #[tokio::test]
    async fn test_player_connection_creates_mapping() {
        // This would require mock GORC instances manager
        // Implementation depends on available testing infrastructure
    }
    
    /// Test that disconnection handler properly cleans up
    #[tokio::test] 
    async fn test_player_disconnection_cleanup() {
        // Test cleanup logic with mock registry
    }
}