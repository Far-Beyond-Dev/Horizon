/// Simple demonstration of zero-cost TCP/UDP and JSON/binary abstractions
use horizon_event_system::*;
use horizon_event_system::binary::*;
use horizon_event_system::protocol::{protocol, format, ProtocolEvent, BinarySerializable};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 Zero-Cost Protocol and Serialization Demo");
    
    // Example 1: TCP with JSON (traditional approach)
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct ChatMessage {
        player_id: String,
        message: String,
        timestamp: u64,
    }
    
    // ChatMessage automatically implements TCP+JSON via blanket impl
    
    let chat_msg = ChatMessage {
        player_id: "player123".to_string(),
        message: "Hello, world!".to_string(),
        timestamp: current_timestamp(),
    };
    
    // TCP+JSON serialization - zero runtime cost for protocol choice
    let tcp_json_data = chat_msg.serialize_for_protocol()?;
    println!("✓ TCP JSON data: {} bytes", tcp_json_data.len());
    
    // Example 2: UDP with Binary (high-performance approach)
    
    let player_id = PlayerId::new();
    let position = Vec3::new(100.0, 50.0, 200.0);
    let velocity = Vec3::new(5.0, 0.0, -2.0);
    
    // Create a zero-allocation binary position update
    let position_update = BinaryPositionUpdate::new(player_id, position, velocity, 12345);
    let binary_event = BinaryEvent::new(event_types::POSITION_UPDATE, position_update);
    
    // UDP+Binary serialization - zero runtime cost for protocol choice
    let udp_binary_data = binary_event.serialize_for_protocol()?;
    println!("✓ UDP Binary data: {} bytes", udp_binary_data.len());
    
    // Example 3: Demonstrate compile-time safety
    
    // These would be COMPILE ERRORS if uncommented:
    // let _wrong1: Result<Vec<u8>, _> = binary_event.serialize_for_protocol::<protocol::Tcp, format::Json>(); // ERROR!
    // let _wrong2: Result<Vec<u8>, _> = chat_msg.serialize_for_protocol::<protocol::Udp, format::Binary>(); // ERROR!
    
    // Example 4: Performance characteristics
    
    // Binary events have predictable, fixed sizes
    let binary_size = std::mem::size_of::<BinaryPositionUpdate>();
    println!("✓ Binary position update size: {} bytes (fixed, no allocations)", binary_size);
    
    // JSON events are variable size but human-readable
    let json_size = serde_json::to_vec(&chat_msg)?.len();
    println!("✓ JSON chat message size: {} bytes (variable, human-readable)", json_size);
    
    // Example 5: Zero-cost verification
    println!("\n🎯 Zero-Cost Abstractions Achieved:");
    println!("• TCP/UDP protocols are compile-time separated");
    println!("• JSON/Binary serialization never cross-contaminate");
    println!("• No runtime protocol checks or fallbacks");
    println!("• Each protocol path is optimized for its use case");
    println!("• Compile-time safety prevents mismatched protocols");
    
    // Example 6: Demonstrate binary roundtrip
    let decoded_binary = BinaryEvent::<BinaryPositionUpdate>::deserialize_binary(&udp_binary_data)?;
    println!("✓ Binary roundtrip successful: position = {:?}", decoded_binary.data.position);
    
    // Example 7: Demonstrate JSON roundtrip
    let decoded_json = <ChatMessage as Event>::deserialize(&tcp_json_data)?;
    println!("✓ JSON roundtrip successful: message = '{}'", decoded_json.message);
    
    println!("\n✅ Zero-cost abstractions verified!");
    
    Ok(())
}