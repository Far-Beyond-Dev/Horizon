/// Simple UDP Event System Example
/// Demonstrates UDP socket event functionality with the new zero-cost abstractions
use horizon_event_system::*;
use horizon_event_system::binary::*;
use std::sync::Arc;

/// Example player movement event - uses automatic Event implementation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct PlayerMovementEvent {
    pub player_id: PlayerId,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub timestamp: u64,
}

/// Example chat message event - uses automatic Event implementation  
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ChatMessageEvent {
    pub sender_id: PlayerId,
    pub message: String,
    pub channel: String,
    pub timestamp: u64,
}

/// Example authentication event - uses automatic Event implementation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct AuthenticationEvent {
    pub player_id: PlayerId,
    pub token: String,
    pub success: bool,
    pub timestamp: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting UDP Event System Example");

    // Create server UDP system
    let server_addr = "127.0.0.1:8080".parse()?;
    let server_udp = Arc::new(UdpEventSystem::new(server_addr, true, 128).await?);
    
    // Start UDP system
    server_udp.start().await?;
    println!("✓ UDP server started on {}", server_addr);

    // Create some test events
    let player_id = PlayerId::new();
    
    // Traditional JSON event (would use TCP in production)
    let movement_event = PlayerMovementEvent {
        player_id,
        position: [100.0, 50.0, 200.0],
        velocity: [5.0, 0.0, -2.0],
        timestamp: current_timestamp(),
    };
    
    let chat_event = ChatMessageEvent {
        sender_id: player_id,
        message: "Hello UDP world!".to_string(),
        channel: "general".to_string(),
        timestamp: current_timestamp(),
    };
    
    let auth_event = AuthenticationEvent {
        player_id,
        token: "auth_token_123".to_string(),
        success: true,
        timestamp: current_timestamp(),
    };

    // Test JSON serialization (for traditional events)
    let movement_json = <PlayerMovementEvent as Event>::serialize(&movement_event)?;
    let chat_json = <ChatMessageEvent as Event>::serialize(&chat_event)?;
    let auth_json = <AuthenticationEvent as Event>::serialize(&auth_event)?;
    
    println!("✓ JSON serialization:");
    println!("  Movement: {} bytes", movement_json.len());
    println!("  Chat: {} bytes", chat_json.len());
    println!("  Auth: {} bytes", auth_json.len());

    // Test binary events (zero-cost UDP approach)
    let binary_position = BinaryPositionUpdate::new(
        player_id,
        Vec3::new(100.0, 50.0, 200.0),
        Vec3::new(5.0, 0.0, -2.0),
        12345
    );
    
    let binary_action = BinaryActionEvent::new(
        player_id,
        1, // action type
        0x12345678, // action data
        100 // timestamp offset
    );

    let position_event = BinaryEvent::new(event_types::POSITION_UPDATE, binary_position);
    let action_event = BinaryEvent::new(event_types::ACTION_EVENT, binary_action);

    // Test binary serialization
    let position_binary = position_event.serialize_binary()?;
    let action_binary = action_event.serialize_binary()?;
    
    println!("✓ Binary serialization:");
    println!("  Position: {} bytes (fixed size)", position_binary.len());
    println!("  Action: {} bytes (fixed size)", action_binary.len());

    // Test binary deserialization roundtrip
    let decoded_position = BinaryEvent::<BinaryPositionUpdate>::deserialize_binary(&position_binary)?;
    let decoded_action = BinaryEvent::<BinaryActionEvent>::deserialize_binary(&action_binary)?;
    
    println!("✓ Binary roundtrip successful:");
    println!("  Position: {:?}", decoded_position.data.position);
    println!("  Action type: {}", decoded_action.data.action_type);

    // Test JSON deserialization roundtrip
    let decoded_movement = <PlayerMovementEvent as Event>::deserialize(&movement_json)?;
    let decoded_chat = <ChatMessageEvent as Event>::deserialize(&chat_json)?;
    
    println!("✓ JSON roundtrip successful:");
    println!("  Movement player: {}", decoded_movement.player_id);
    println!("  Chat message: '{}'", decoded_chat.message);

    // Add a mock UDP connection for demonstration
    let client_addr = "127.0.0.1:9000".parse()?;
    server_udp.add_udp_connection(client_addr, player_id, "test_conn".to_string()).await;
    println!("✓ Mock UDP connection added");

    // Demonstrate raw UDP sending (what the protocol system would use)
    if let Err(e) = server_udp.send_raw_to_addr(client_addr, "position_update", &position_binary).await {
        println!("⚠ UDP send simulation (expected error): {}", e);
    }

    // Get UDP statistics
    let stats = server_udp.get_udp_stats().await;
    println!("✓ UDP Stats: {} connections, {} bytes sent", 
             stats.connection_count, stats.total_bytes_sent);

    // Performance comparison
    println!("\n📊 Performance Comparison:");
    println!("JSON Events (TCP-optimized):");
    println!("  Variable size, human-readable");
    println!("  Movement: {} bytes", movement_json.len());
    println!("  Chat: {} bytes", chat_json.len());
    
    println!("Binary Events (UDP-optimized):");
    println!("  Fixed size, zero-allocation");
    println!("  Position: {} bytes", position_binary.len());
    println!("  Action: {} bytes", action_binary.len());
    
    println!("\n🎯 Zero-Cost Abstractions Demonstrated:");
    println!("• TCP/UDP protocols completely separated at compile time");
    println!("• JSON/Binary serialization never cross-contaminate");
    println!("• Each protocol optimized for its specific use case");
    println!("• No runtime overhead from abstraction layer");

    // Cleanup
    server_udp.remove_udp_connection(client_addr).await;
    server_udp.stop().await;
    
    println!("✅ UDP Event System Example completed successfully!");
    
    Ok(())
}