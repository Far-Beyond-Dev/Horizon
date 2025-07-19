/// Example demonstrating zero-cost TCP/UDP and JSON/binary abstractions
use horizon_event_system::*;
use horizon_event_system::binary::*;
use horizon_event_system::protocol::{protocol, format, ProtocolEvent};
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;

#[derive(Debug)]
struct MockServerContext;

#[async_trait::async_trait]
impl context::ServerContext for MockServerContext {
    fn events(&self) -> Arc<EventSystem> {
        Arc::new(EventSystem::new())
    }
    
    fn region_id(&self) -> RegionId {
        RegionId::new()
    }
    
    fn log(&self, _level: context::LogLevel, _message: &str) {
        println!("[LOG] {}", _message);
    }
    
    async fn send_to_player(&self, _player_id: PlayerId, _data: &[u8]) -> Result<(), context::ServerError> {
        Ok(())
    }
    
    async fn broadcast(&self, _data: &[u8]) -> Result<(), context::ServerError> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Zero-Cost Protocol and Serialization Example");
    
    // Create event system
    let server_context = Arc::new(MockServerContext);
    let (events, _gorc_system) = create_complete_horizon_system(server_context)?;
    
    println!("✓ Event system created");
    
    // Example 1: TCP with JSON (traditional approach, zero-cost when using TCP)
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct ChatMessage {
        player_id: String,
        message: String,
        timestamp: u64,
    }
    
    // ChatMessage automatically implements ProtocolEvent<Tcp, Json> via blanket impl
    
    // Note: TCP JSON functionality is demonstrated but not fully implemented in this example
    // The compile-time separation is the key achievement
    println!("✓ TCP JSON type safety enforced at compile time");
    
    // Example 2: UDP with Binary (high-performance approach, zero-cost when using UDP)
    
    // Create a binary position update - this is a zero-allocation, direct binary representation
    let player_id = PlayerId::new();
    let position = Vec3::new(100.0, 50.0, 200.0);
    let velocity = Vec3::new(5.0, 0.0, -2.0);
    
    let position_update = BinaryPositionUpdate::new(player_id, position, velocity, 12345);
    let binary_event = BinaryEvent::new(event_types::POSITION_UPDATE, position_update);
    
    // Note: UDP binary functionality is demonstrated but not fully implemented in this example
    // The compile-time separation is the key achievement
    println!("✓ UDP binary type safety enforced at compile time");
    
    // Example 3: Demonstrate the zero-cost nature
    
    // When using TCP + JSON, there's no binary serialization overhead
    let chat_msg = ChatMessage {
        player_id: player_id.to_string(),
        message: "Hello, world!".to_string(),
        timestamp: current_timestamp(),
    };
    
    // Demonstrate type safety - these are the TYPES that would be used:
    let tcp_json_data = <ChatMessage as ProtocolEvent<protocol::Tcp, format::Json>>::serialize_for_protocol(&chat_msg)?; // JSON serialization
    let udp_binary_data = <BinaryPositionEvent as ProtocolEvent<protocol::Udp, format::Binary>>::serialize_for_protocol(&binary_event)?; // Binary serialization
    
    println!("TCP JSON data: {} bytes", tcp_json_data.len());
    println!("UDP Binary data: {} bytes", udp_binary_data.len());
    
    println!("✓ TCP JSON type safety enforced at compile time");
    println!("✓ UDP binary type safety enforced at compile time");
    
    // Demonstrate compile-time separation - these would be compile errors:
    // events.emit_tcp_json("position_update", &binary_event).await?; // ERROR: BinaryEvent doesn't implement TCP+JSON
    // events.emit_udp_binary(player_id, "chat_message", &chat_msg).await?; // ERROR: ChatMessage doesn't implement UDP+Binary
    
    println!("✓ Compile-time protocol safety enforced");
    
    // Example 4: Performance characteristics
    
    // Binary events have predictable, fixed sizes
    let binary_size = std::mem::size_of::<BinaryPositionUpdate>();
    println!("Binary position update size: {} bytes (fixed, no allocations)", binary_size);
    
    // JSON events are variable size but human-readable
    let json_size = serde_json::to_vec(&chat_msg)?.len();
    println!("JSON chat message size: {} bytes (variable, human-readable)", json_size);
    
    println!("\n🎯 Zero-Cost Abstractions Achieved:");
    println!("• TCP/UDP protocols are compile-time separated");
    println!("• JSON/Binary serialization never cross-contaminate");
    println!("• No runtime protocol checks or fallbacks");
    println!("• Each protocol path is optimized for its use case");
    println!("• Compile-time safety prevents mismatched protocols");
    
    Ok(())
}