/// UDP Event System Example
/// Demonstrates complete UDP socket event functionality with binary serialization
use horizon_event_system::*;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use serde::{Serialize, Deserialize};

/// Example player movement event for UDP transmission
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerMovementEvent {
    pub player_id: PlayerId,
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub timestamp: u64,
}

impl Event for PlayerMovementEvent {
    fn serialize(&self) -> Result<String, EventError> {
        serde_json::to_string(self)
            .map_err(|e| EventError::HandlerExecution(format!("Serialization failed: {}", e)))
    }

    fn type_name(&self) -> &'static str {
        "PlayerMovementEvent"
    }
}

/// Example chat message event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessageEvent {
    pub sender_id: PlayerId,
    pub message: String,
    pub channel: String,
    pub timestamp: u64,
}

impl Event for ChatMessageEvent {
    fn serialize(&self) -> Result<String, EventError> {
        serde_json::to_string(self)
            .map_err(|e| EventError::HandlerExecution(format!("Serialization failed: {}", e)))
    }

    fn type_name(&self) -> &'static str {
        "ChatMessageEvent"
    }
}

/// Example authentication event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthenticationEvent {
    pub player_id: PlayerId,
    pub token: String,
    pub success: bool,
    pub timestamp: u64,
}

impl Event for AuthenticationEvent {
    fn serialize(&self) -> Result<String, EventError> {
        serde_json::to_string(self)
            .map_err(|e| EventError::HandlerExecution(format!("Serialization failed: {}", e)))
    }

    fn type_name(&self) -> &'static str {
        "AuthenticationEvent"
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("🚀 Starting UDP Event System Example");

    // Create server UDP system
    let server_addr = "127.0.0.1:8080".parse()?;
    let server_udp = Arc::new(UdpEventSystem::new(server_addr, true, 128).await?);
    
    // Create client UDP system
    let client_addr = "127.0.0.1:8081".parse()?;
    let client_udp = Arc::new(UdpEventSystem::new(client_addr, true, 128).await?);

    // Create reliability manager for both
    let server_reliability = Arc::new(UdpReliabilityManager::new(
        3, // max retries
        Duration::from_millis(100), // base RTO
        1000, // max sequence buffer size
    ));
    
    let client_reliability = Arc::new(UdpReliabilityManager::new(
        3,
        Duration::from_millis(100),
        1000,
    ));

    // Start both systems
    server_udp.start().await?;
    client_udp.start().await?;
    server_reliability.start().await;
    client_reliability.start().await;

    println!("📡 UDP systems started on {} and {}", server_addr, client_addr);

    // Register event codecs on both systems
    server_udp.register_json_codec::<PlayerMovementEvent>("player_movement").await;
    server_udp.register_json_codec::<ChatMessageEvent>("chat_message").await;
    server_udp.register_json_codec::<AuthenticationEvent>("authentication").await;
    
    client_udp.register_json_codec::<PlayerMovementEvent>("player_movement").await;
    client_udp.register_json_codec::<ChatMessageEvent>("chat_message").await;
    client_udp.register_json_codec::<AuthenticationEvent>("authentication").await;

    println!("🔧 Registered binary event codecs");

    // Register event handlers on server
    let server_udp_clone = server_udp.clone();
    server_udp.register_udp_handler("player_movement", move |packet, connection, data| {
        println!("🏃 Server received player movement from {}", connection.addr);
        
        // Parse the movement data
        if let Ok(movement_str) = std::str::from_utf8(data) {
            if let Ok(movement) = serde_json::from_str::<PlayerMovementEvent>(movement_str) {
                println!("   Player {} moved to {:?}", movement.player_id, movement.position);
                
                // Echo back a response
                let response = format!("Movement confirmed for player {}", movement.player_id);
                return Ok(Some(response.into_bytes()));
            }
        }
        
        Ok(None)
    }).await?;

    server_udp.register_udp_handler("chat_message", |packet, connection, data| {
        println!("💬 Server received chat message from {}", connection.addr);
        
        if let Ok(chat_str) = std::str::from_utf8(data) {
            if let Ok(chat) = serde_json::from_str::<ChatMessageEvent>(chat_str) {
                println!("   {} says: '{}'", chat.sender_id, chat.message);
            }
        }
        
        Ok(Some(b"Message received".to_vec()))
    }).await?;

    server_udp.register_udp_handler("authentication", |packet, connection, data| {
        println!("🔐 Server received authentication from {}", connection.addr);
        
        if let Ok(auth_str) = std::str::from_utf8(data) {
            if let Ok(auth) = serde_json::from_str::<AuthenticationEvent>(auth_str) {
                println!("   Player {} authentication: {}", auth.player_id, auth.success);
                connection.state = if auth.success { 
                    UdpConnectionState::Connected 
                } else { 
                    UdpConnectionState::Disconnecting 
                };
            }
        }
        
        Ok(Some(b"Auth processed".to_vec()))
    }).await?;

    // Register handlers on client
    client_udp.register_udp_handler("response", |packet, connection, data| {
        if let Ok(response_str) = std::str::from_utf8(data) {
            println!("📥 Client received response: {}", response_str);
        }
        Ok(None)
    }).await?;

    println!("🎯 Registered UDP event handlers");

    // Simulate client connecting to server
    let player1_id = PlayerId::new();
    let player2_id = PlayerId::new();
    
    // Add connections
    server_udp.add_udp_connection(client_addr, player1_id, "client1".to_string()).await;
    client_udp.add_udp_connection(server_addr, player1_id, "server".to_string()).await;

    println!("🔗 Established UDP connections");

    // Demonstrate authentication event (reliable)
    println!("\n=== Testing Authentication (Reliable) ===");
    let auth_event = AuthenticationEvent {
        player_id: player1_id,
        token: "secure_token_123".to_string(),
        success: true,
        timestamp: current_timestamp(),
    };

    client_udp.send_udp_event_to_addr(server_addr, "authentication", &auth_event).await?;
    sleep(Duration::from_millis(100)).await;

    // Demonstrate chat message (ordered)
    println!("\n=== Testing Chat Messages (Ordered) ===");
    for i in 1..=3 {
        let chat_event = ChatMessageEvent {
            sender_id: player1_id,
            message: format!("Hello from player! Message #{}", i),
            channel: "global".to_string(),
            timestamp: current_timestamp(),
        };

        client_udp.send_udp_event_to_addr(server_addr, "chat_message", &chat_event).await?;
        sleep(Duration::from_millis(50)).await;
    }

    // Demonstrate movement events (sequenced)
    println!("\n=== Testing Movement Events (Sequenced) ===");
    for i in 0..5 {
        let movement_event = PlayerMovementEvent {
            player_id: player1_id,
            position: [i as f32 * 10.0, 0.0, 0.0],
            velocity: [1.0, 0.0, 0.0],
            timestamp: current_timestamp(),
        };

        client_udp.send_udp_event_to_addr(server_addr, "player_movement", &movement_event).await?;
        sleep(Duration::from_millis(30)).await;
    }

    // Test broadcasting
    println!("\n=== Testing Broadcast ===");
    let broadcast_chat = ChatMessageEvent {
        sender_id: player2_id,
        message: "Server announcement to all players!".to_string(),
        channel: "announcement".to_string(),
        timestamp: current_timestamp(),
    };

    server_udp.broadcast_udp_event("chat_message", &broadcast_chat).await?;
    sleep(Duration::from_millis(100)).await;

    // Test large event (compression)
    println!("\n=== Testing Large Event (Compression) ===");
    let large_message = "A".repeat(2000); // 2KB message to trigger compression
    let large_chat = ChatMessageEvent {
        sender_id: player1_id,
        message: large_message,
        channel: "large".to_string(),
        timestamp: current_timestamp(),
    };

    client_udp.send_udp_event_to_addr(server_addr, "chat_message", &large_chat).await?;
    sleep(Duration::from_millis(100)).await;

    // Display statistics
    println!("\n=== UDP Statistics ===");
    let server_stats = server_udp.get_udp_stats().await;
    let client_stats = client_udp.get_udp_stats().await;
    let server_reliability_stats = server_reliability.get_reliability_stats().await;
    let client_reliability_stats = client_reliability.get_reliability_stats().await;

    println!("Server UDP Stats:");
    println!("  Connections: {}", server_stats.connection_count);
    println!("  Bytes sent: {}", server_stats.total_bytes_sent);
    println!("  Bytes received: {}", server_stats.total_bytes_received);
    println!("  Send buffer: {} bytes", server_stats.send_buffer_size);
    println!("  Recv buffer: {} bytes", server_stats.recv_buffer_size);

    println!("Client UDP Stats:");
    println!("  Connections: {}", client_stats.connection_count);
    println!("  Bytes sent: {}", client_stats.total_bytes_sent);
    println!("  Bytes received: {}", client_stats.total_bytes_received);

    println!("Server Reliability Stats:");
    println!("  Pending packets: {}", server_reliability_stats.pending_packets);
    println!("  Active connections: {}", server_reliability_stats.active_connections);
    println!("  Sequence buffers: {}", server_reliability_stats.sequence_buffers);
    println!("  Max retries: {}", server_reliability_stats.max_retries);
    println!("  Base RTO: {}ms", server_reliability_stats.base_rto_ms);

    // Test reliability modes
    println!("\n=== Reliability Mode Examples ===");
    println!("player_connect -> {:?}", get_reliability_mode_for_event("player_connect"));
    println!("position_update -> {:?}", get_reliability_mode_for_event("position_update"));
    println!("chat_message -> {:?}", get_reliability_mode_for_event("chat_message"));
    println!("random_event -> {:?}", get_reliability_mode_for_event("random_event"));

    // Integration with EventSystem
    println!("\n=== EventSystem Integration ===");
    let mut event_system = EventSystem::new();
    event_system.set_udp_system(server_udp.clone());

    // Use extension trait methods
    let event_system = Arc::new(event_system);
    
    event_system.on_udp("test_integration", |packet, connection, data| {
        println!("🔌 Integrated UDP handler called from EventSystem");
        Ok(None)
    }).await?;

    let integration_event = ChatMessageEvent {
        sender_id: player1_id,
        message: "Testing EventSystem integration".to_string(),
        channel: "integration".to_string(),
        timestamp: current_timestamp(),
    };

    // This would work if we had the proper integration setup
    if let Some(udp_stats) = event_system.get_udp_stats().await {
        println!("EventSystem UDP integration working: {} connections", udp_stats.connection_count);
    }

    // Allow time for final processing
    sleep(Duration::from_millis(200)).await;

    // Clean shutdown
    println!("\n🛑 Shutting down UDP systems");
    server_reliability.stop().await;
    client_reliability.stop().await;
    server_udp.stop().await;
    client_udp.stop().await;

    println!("✅ UDP Event System example completed successfully!");
    Ok(())
}

/// Helper function to demonstrate custom binary serializer
#[allow(dead_code)]
pub struct CustomBinarySerializer;

impl BinaryEventSerializer for CustomBinarySerializer {
    fn serialize(&self, event: &dyn Event) -> Result<Vec<u8>, EventError> {
        // Custom binary format (example: simple length-prefixed JSON)
        let json_data = event.serialize()?.into_bytes();
        let length = (json_data.len() as u32).to_le_bytes();
        
        let mut result = Vec::with_capacity(4 + json_data.len());
        result.extend_from_slice(&length);
        result.extend_from_slice(&json_data);
        
        Ok(result)
    }

    fn event_type(&self) -> &str {
        "CustomEvent"
    }
}

/// Helper function to demonstrate custom binary deserializer
#[allow(dead_code)]
pub struct CustomBinaryDeserializer;

impl BinaryEventDeserializer for CustomBinaryDeserializer {
    fn deserialize(&self, data: &[u8]) -> Result<Box<dyn Event>, EventError> {
        if data.len() < 4 {
            return Err(EventError::HandlerExecution("Data too short".to_string()));
        }
        
        let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + length {
            return Err(EventError::HandlerExecution("Invalid length".to_string()));
        }
        
        let json_data = &data[4..4 + length];
        let json_str = String::from_utf8(json_data.to_vec())
            .map_err(|e| EventError::HandlerExecution(format!("UTF-8 decode error: {}", e)))?;
        
        // For this example, we'll deserialize as a chat message
        let event: ChatMessageEvent = serde_json::from_str(&json_str)
            .map_err(|e| EventError::HandlerExecution(format!("JSON decode error: {}", e)))?;
        
        Ok(Box::new(event))
    }

    fn event_type(&self) -> &str {
        "CustomEvent"
    }
}