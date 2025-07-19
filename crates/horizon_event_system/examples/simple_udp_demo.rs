/// Simple UDP demo using the event system
use horizon_event_system::*;
use horizon_event_system::binary::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple UDP Demo");
    
    // Create UDP system
    let server_addr = "127.0.0.1:8080".parse()?;
    let udp_system = Arc::new(UdpEventSystem::new(server_addr, true, 128).await?);
    
    println!("✓ UDP system created on {}", server_addr);
    
    // Start the UDP system
    udp_system.start().await?;
    println!("✓ UDP system started");
    
    // Create some binary events
    let player_id = PlayerId::new();
    
    // Binary position event
    let position_update = BinaryPositionUpdate::new(
        player_id,
        Vec3::new(100.0, 50.0, 200.0),
        Vec3::new(5.0, 0.0, -2.0),
        12345
    );
    let binary_event = BinaryEvent::new(event_types::POSITION_UPDATE, position_update);
    
    // Serialize binary event
    let binary_data = binary_event.serialize_binary()?;
    println!("✓ Binary event serialized: {} bytes", binary_data.len());
    
    // Deserialize binary event to verify roundtrip
    let decoded = BinaryEvent::<BinaryPositionUpdate>::deserialize_binary(&binary_data)?;
    println!("✓ Binary event decoded: position = {:?}", decoded.data.position);
    
    // Binary action event
    let action_event = BinaryActionEvent::new(player_id, 1, 0x12345678, 100);
    let action_binary = BinaryEvent::new(event_types::ACTION_EVENT, action_event);
    
    let action_data = action_binary.serialize_binary()?;
    println!("✓ Action event serialized: {} bytes", action_data.len());
    
    // Add a mock connection for demonstration
    let client_addr = "127.0.0.1:9000".parse()?;
    udp_system.add_udp_connection(client_addr, player_id, "conn_123".to_string()).await;
    println!("✓ Mock UDP connection added");
    
    // Simulate sending raw binary data
    if let Err(e) = udp_system.send_raw_to_addr(client_addr, "position_update", &binary_data).await {
        println!("⚠ UDP send simulation: {}", e);
    } else {
        println!("✓ UDP binary data sent");
    }
    
    // Get UDP stats
    let stats = udp_system.get_udp_stats().await;
    println!("✓ UDP Stats: {} connections, {} bytes sent, {} bytes received", 
             stats.connection_count, stats.total_bytes_sent, stats.total_bytes_received);
    
    // Clean up
    udp_system.remove_udp_connection(client_addr).await;
    udp_system.stop().await;
    
    println!("\n🎯 UDP Demo Summary:");
    println!("• Binary events provide zero-allocation serialization");
    println!("• Fixed-size events enable predictable performance");
    println!("• UDP system supports compression and reliability");
    println!("• Type-safe event handling with compile-time guarantees");
    
    Ok(())
}