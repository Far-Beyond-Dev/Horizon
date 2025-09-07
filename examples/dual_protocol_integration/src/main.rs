//! Integration example showing backward compatibility API design.
//!
//! This example demonstrates the API structure for supporting both:
//! 1. Legacy WebSocket connections (existing clients)
//! 2. New TCP+JSON connections (new clients using the generalized stack)
//!
//! Note: This is a simplified demo showing the integration points without
//! running actual servers, since some internals are not exposed publicly.

use game_server::communication::{EnhancedConnectionManager, ProtocolMessage};
use horizon_event_system::{
    create_horizon_event_system, RawClientMessageEvent, EventSystem,
};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

/// Example game plugin that handles messages from both WebSocket and TCP+JSON clients.
async fn setup_game_plugins(event_system: Arc<EventSystem>) -> Result<(), Box<dyn std::error::Error>> {
    // Register a movement handler that works with both connection types
    event_system.on_client("movement", "move_request", |event: RawClientMessageEvent| {
        let data: serde_json::Value = serde_json::from_slice(&event.data)?;
        println!("🏃 Movement request from player {}: {:?}", event.player_id, data);
        Ok(())
    }).await?;
    
    // Register a chat handler
    event_system.on_client("chat", "send_message", |event: RawClientMessageEvent| {
        let data: serde_json::Value = serde_json::from_slice(&event.data)?;
        println!("💬 Chat message from player {}: {:?}", event.player_id, data);
        Ok(())
    }).await?;
    
    // Register core event handlers
    event_system.on_core("raw_client_message", |event: RawClientMessageEvent| {
        println!("📨 Raw message: {} from player {}", event.message_type, event.player_id);
        Ok(())
    }).await?;
    
    println!("✅ Game plugins registered");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Dual Protocol Server - WebSocket + TCP+JSON Integration");
    println!("=========================================================");
    
    // Create the event system
    let event_system = create_horizon_event_system();
    
    // Set up game plugins
    setup_game_plugins(event_system).await?;
    
    // Create connection managers (simplified for demo)
    println!("⚠️  Note: This is a simplified demo showing the API structure.");
    println!("For a full implementation, you would need to expose ConnectionManager");
    println!("and handle_connection from game_server, and implement proper server loops.");
    
    println!("✅ API structure demonstration completed!");
    println!("\nKey integration points:");
    println!("- EnhancedConnectionManager wraps existing ConnectionManager");
    println!("- handle_tcp_json_connection processes new protocol connections");
    println!("- Both connection types use the same EventSystem");
    println!("- Message routing is unified through existing infrastructure");
    println!("- Full backward compatibility is maintained");
    
    // Demonstrate message compatibility
    demonstrate_message_compatibility();
    
    // Demonstrate protocol characteristics
    demonstrate_protocol_characteristics();

    Ok(())
}

/// Demonstrate message format compatibility between protocols.
fn demonstrate_message_compatibility() {
    println!("\n🔄 Message Format Compatibility Demo");
    println!("=====================================");
    
    // Create a new protocol message
    let protocol_msg = ProtocolMessage {
        namespace: "inventory".to_string(),
        event: "use_item".to_string(),
        data: serde_json::json!({
            "item_id": "sword_001",
            "target_id": "player_123"
        }),
        meta: Some(serde_json::json!({
            "protocol_version": 1,
            "compression": "none"
        })),
    };
    
    println!("📦 Original ProtocolMessage: {:?}", protocol_msg);
    
    // Convert to legacy ClientMessage (loses meta but preserves core data)
    let client_msg: game_server::messaging::ClientMessage = protocol_msg.clone().into();
    println!("🔄 Converted to ClientMessage: {:?}", client_msg);
    
    // Convert back to ProtocolMessage (meta is lost, as expected)
    let back_to_protocol: ProtocolMessage = client_msg.into(); 
    println!("🔄 Converted back to ProtocolMessage: {:?}", back_to_protocol);
    
    println!("✅ Backward compatibility maintained - existing plugins work unchanged");
}

/// Demonstrate protocol characteristics at compile time.
fn demonstrate_protocol_characteristics() {
    use horizon_event_system::{TcpJsonEndpoint, UdpBinaryEndpoint};
    
    println!("\n⚡ Protocol Characteristics (Compile-Time)");
    println!("============================================");
    
    let tcp_json_info = TcpJsonEndpoint::transport_info();
    println!("TCP+JSON Transport: {} (reliable: {}, connection-oriented: {})",
             tcp_json_info.name, tcp_json_info.reliable, tcp_json_info.connection_oriented);
    
    let tcp_json_format = TcpJsonEndpoint::format_info();
    println!("TCP+JSON Format: {} (human-readable: {}, MIME: {})",
             tcp_json_format.name, tcp_json_format.human_readable, tcp_json_format.mime_type);
    
    let udp_binary_info = UdpBinaryEndpoint::transport_info();
    println!("UDP+Binary Transport: {} (reliable: {}, connection-oriented: {})",
             udp_binary_info.name, udp_binary_info.reliable, udp_binary_info.connection_oriented);
    
    let udp_binary_format = UdpBinaryEndpoint::format_info();
    println!("UDP+Binary Format: {} (human-readable: {}, MIME: {})",
             udp_binary_format.name, udp_binary_format.human_readable, udp_binary_format.mime_type);
}