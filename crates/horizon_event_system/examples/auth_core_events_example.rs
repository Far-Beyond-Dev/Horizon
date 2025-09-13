//! Example demonstrating authentication event usage in plugins.
//!
//! This example shows how to use the new authentication core events to manage
//! player authentication state in a plugin-based architecture.

use horizon_event_system::{
    PlayerId, AuthenticationStatus, AuthenticationStatusSetEvent,
    AuthenticationStatusGetEvent, AuthenticationStatusGetResponseEvent, 
    AuthenticationStatusChangedEvent, current_timestamp,
    create_horizon_event_system, RawClientMessageEvent
};
use tokio;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    info!("🚀 Starting Authentication Event Example");
    
    // Create the event system
    let events = create_horizon_event_system();
    
    // Register authentication event handlers
    info!("✅ Registering authentication event handlers...");
    
    // Handle authentication status set events
    events.on_core("auth_status_set", |event: AuthenticationStatusSetEvent| {
        info!("📝 Auth status set for player {}: {:?}", event.player_id, event.status);
        Ok(())
    }).await?;
    
    // Handle authentication status query events
    events.on_core("auth_status_get", |event: AuthenticationStatusGetEvent| {
        info!("🔍 Auth status query for player: {} (request: {})", 
                event.player_id, event.request_id);
        // In a real plugin, this would query the actual auth status and respond
        Ok(())
    }).await?;

    // Handle authentication status query responses
    events.on_core("auth_status_get_response", |event: AuthenticationStatusGetResponseEvent| {
        info!("📨 Auth status response for player {}: {:?} (request: {})", 
                event.player_id, event.status, event.request_id);
        Ok(())
    }).await?;

    // Handle authentication status change notifications
    events.on_core("auth_status_changed", |event: AuthenticationStatusChangedEvent| {
        info!("🔄 Auth status changed for player {}: {:?} -> {:?}", 
                event.player_id, event.old_status, event.new_status);
        Ok(())
    }).await?;
    
    // Register a game logic handler that demonstrates auth status checking
    events.on_client("game", "move_request", |event: RawClientMessageEvent, player_id: horizon_event_system::PlayerId, connection: horizon_event_system::ClientConnectionRef| {
        info!("🎮 Move request from player: {}", event.player_id);
        
        // In a real implementation, you would check the auth status before processing
        // This could be done by:
        // 1. Looking up the player's auth status in a shared state
        // 2. Using the ClientResponseSender to check auth status
        // 3. Querying an authentication service
        
        // For demonstration, let's simulate an auth check
        let auth_status_ok = true; // This would be a real check
        
        if auth_status_ok {
            info!("✅ Player {} is authenticated - processing move request", event.player_id);
            // Process the move request...
        } else {
            info!("❌ Player {} is not authenticated - rejecting move request", event.player_id);
            // Reject the request or trigger re-authentication
        }
        
        Ok(())
    }).await?;
    
    info!("✅ Event handlers registered");
    
    // Simulate some authentication events
    let player_id = PlayerId::new();
    info!("\n📋 Simulating authentication workflow for player: {}", player_id);
    
    // Step 1: Set initial authentication status to Authenticating
    info!("\n1️⃣ Setting authentication status to Authenticating...");
    events.emit_core("auth_status_set", &AuthenticationStatusSetEvent {
        player_id,
        status: AuthenticationStatus::Authenticating,
        timestamp: current_timestamp(),
    }).await?;
    
    // Step 2: Query authentication status
    info!("\n2️⃣ Querying authentication status...");
    events.emit_core("auth_status_get", &AuthenticationStatusGetEvent {
        player_id,
        request_id: "example_query_123".to_string(),
        timestamp: current_timestamp(),
    }).await?;
    
    // Step 3: Simulate successful authentication
    info!("\n3️⃣ Authentication successful - updating status...");
    events.emit_core("auth_status_set", &AuthenticationStatusSetEvent {
        player_id,
        status: AuthenticationStatus::Authenticated,
        timestamp: current_timestamp(),
    }).await?;
    
    // Step 4: Simulate authentication status change notification
    info!("\n4️⃣ Simulating auth status change notification...");
    events.emit_core("auth_status_changed", &AuthenticationStatusChangedEvent {
        player_id,
        old_status: AuthenticationStatus::Authenticating,
        new_status: AuthenticationStatus::Authenticated,
        timestamp: current_timestamp(),
    }).await?;
    
    // Step 5: Simulate a game action from an authenticated player
    info!("\n5️⃣ Simulating game action from authenticated player...");
    events.emit_client("game", "move_request", &RawClientMessageEvent {
        player_id,
        message_type: "move_request".to_string(),
        data: b"{\"x\": 100, \"y\": 200, \"z\": 150}".to_vec(),
        timestamp: current_timestamp(),
    }).await?;
    
    // Step 6: Demonstrate authentication failure
    let another_player = PlayerId::new();
    info!("\n6️⃣ Simulating authentication failure for another player...");
    events.emit_core("auth_status_set", &AuthenticationStatusSetEvent {
        player_id: another_player,
        status: AuthenticationStatus::AuthenticationFailed,
        timestamp: current_timestamp(),
    }).await?;
    
    info!("\n🎯 Example completed! Authentication events are working correctly.");
    info!("\n💡 Key benefits:");
    info!("   - ✅ Type-safe authentication status management");
    info!("   - ✅ Clean separation between auth and game logic");
    info!("   - ✅ Event-driven architecture allows plugins to react to auth changes");
    info!("   - ✅ Core events provide standardized auth messaging between plugins");
    info!("   - ✅ Integration with player index allows client-aware handlers to query auth status");
    
    Ok(())
}